use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use rumqttc::{
    AsyncClient, ClientError, Event, EventLoop, Incoming, LastWill, MqttOptions, Outgoing, QoS,
    TlsConfiguration, Transport,
};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::MqttConfig;

#[derive(Debug)]
pub enum MqttError {
    Client(ClientError),
    Config(String),
}

impl std::fmt::Display for MqttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MqttError::Client(e) => write!(f, "{e}"),
            MqttError::Config(e) => write!(f, "MQTT configuration error: {e}"),
        }
    }
}

impl std::error::Error for MqttError {}

impl From<ClientError> for MqttError {
    fn from(e: ClientError) -> Self {
        MqttError::Client(e)
    }
}

#[derive(Debug, Clone)]
pub enum MqttEvent {
    Connected,
    Disconnected,
    Publish { topic: String, payload: Vec<u8> },
    PubAck(u16),
    // Other events can be added as needed
}

#[derive(Clone)]
pub struct MqttService {
    link: Link,
    ready: Arc<AtomicBool>,
    events_tx: broadcast::Sender<MqttEvent>,
    subscriptions: Arc<RwLock<HashMap<String, QoS>>>,
}

#[derive(Clone)]
enum Link {
    Broker {
        client: Arc<Mutex<AsyncClient>>,
        // The event loop: it lives as long as the service, or until `close`
        loop_handle: Arc<JoinHandle<()>>,
    },
    /// No network: a publish goes straight back out as an event when a subscription matches.
    Loopback,
}

impl MqttService {
    #[expect(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "spawns the event loop: async so callers are inside the Tokio runtime"
    )]
    pub async fn connect(config: MqttConfig) -> Result<Self, MqttError> {
        let (client, eventloop) = build_client(&config)?;
        let ready = Arc::new(AtomicBool::new(false));
        let (tx, _) = broadcast::channel(256);
        let subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let ready_clone = ready.clone();
        let tx_clone = tx.clone();
        let subscriptions_clone = subscriptions.clone();

        let client_shared = Arc::new(Mutex::new(client));
        let client_clone = client_shared.clone();
        let loop_handle = tokio::spawn(async move {
            run_eventloop(
                eventloop,
                client_clone,
                ready_clone,
                tx_clone,
                subscriptions_clone,
                config,
            )
            .await;
        });

        Ok(Self {
            link: Link::Broker {
                client: client_shared,
                loop_handle: Arc::new(loop_handle),
            },
            ready,
            events_tx: tx,
            subscriptions,
        })
    }

    /// An in-process broker for tests: always connected, no network. A publish comes back as
    /// a `Publish` event when one of the subscriptions matches its topic (retain is ignored),
    /// followed by a `PubAck` for QoS 1 and 2.
    pub fn loopback() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            link: Link::Loopback,
            ready: Arc::new(AtomicBool::new(true)),
            events_tx: tx,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    pub fn events(&self) -> broadcast::Receiver<MqttEvent> {
        self.events_tx.subscribe()
    }

    pub async fn publish<T: Into<Vec<u8>>>(
        &self,
        topic: &str,
        qos: QoS,
        retain: bool,
        payload: T,
    ) -> Result<(), ClientError> {
        let client = match &self.link {
            Link::Broker { client, .. } => client,
            Link::Loopback => {
                let subscribed = self
                    .subscriptions
                    .read()
                    .await
                    .keys()
                    .any(|filter| topic_matches(filter, topic));
                if subscribed {
                    let _ = self.events_tx.send(MqttEvent::Publish {
                        topic: topic.to_string(),
                        payload: payload.into(),
                    });
                }
                if qos != QoS::AtMostOnce {
                    let _ = self.events_tx.send(MqttEvent::PubAck(0));
                }
                return Ok(());
            }
        };
        let client = client.lock().await;
        client.publish(topic, qos, retain, payload).await
    }

    pub async fn subscribe(&self, topic: &str, qos: QoS) -> Result<(), ClientError> {
        let client = match &self.link {
            Link::Broker { client, .. } => client,
            Link::Loopback => {
                self.subscriptions
                    .write()
                    .await
                    .insert(topic.to_string(), qos);
                return Ok(());
            }
        };
        let client = client.lock().await;
        let result = client.subscribe(topic, qos).await;
        if result.is_ok() {
            // Track successful subscriptions
            let mut subs = self.subscriptions.write().await;
            subs.insert(topic.to_string(), qos);
        }
        result
    }

    pub async fn disconnect(&self) -> Result<(), ClientError> {
        let Link::Broker { client, .. } = &self.link else {
            return Ok(());
        };
        self.ready.store(false, Ordering::Relaxed);
        let client = client.lock().await;
        client.disconnect().await
    }

    /// Disconnect and stop for good: no reconnect. For a client made for a while (Home
    /// Assistant's broker), never the server's own.
    pub async fn close(&self) {
        let Link::Broker { loop_handle, .. } = &self.link else {
            return;
        };
        let _ = self.disconnect().await;
        // The event loop sends the DISCONNECT on its next poll
        sleep(Duration::from_millis(300)).await;
        loop_handle.abort();
        self.ready.store(false, Ordering::Relaxed);
    }

    pub async fn resubscribe_tracked(&self) -> Result<(), ClientError> {
        let Link::Broker { client, .. } = &self.link else {
            return Ok(());
        };
        let subs = self.subscriptions.read().await;
        let client = client.lock().await;
        for (topic, qos) in subs.iter() {
            debug!("Re-subscribing to {} with QoS {:?}", topic, qos);
            if let Err(err) = client.subscribe(topic, *qos).await {
                warn!(?err, "Failed to re-subscribe to {}", topic);
                return Err(err);
            }
            // Small delay between subscriptions to avoid overwhelming the broker
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        info!("Successfully re-subscribed to {} topics", subs.len());
        Ok(())
    }
}

/// MQTT topic filter matching: `+` is one level, a trailing `#` is any number of levels
/// (including none).
fn topic_matches(filter: &str, topic: &str) -> bool {
    let mut levels = topic.split('/');
    for f in filter.split('/') {
        match (f, levels.next()) {
            ("#", _) => return true,
            ("+", Some(_)) => {}
            (f, Some(t)) if f == t => {}
            _ => return false,
        }
    }
    levels.next().is_none()
}

fn build_client(config: &MqttConfig) -> Result<(AsyncClient, EventLoop), MqttError> {
    let mut opts = MqttOptions::new(&config.client_id, &config.host, config.port);
    opts.set_keep_alive(Duration::from_secs(config.keep_alive_secs as u64));
    opts.set_clean_session(config.clean_session);
    // Connection timeout not available in this rumqttc version; rely on defaults
    if let (Some(u), Some(p)) = (&config.username, &config.password) {
        opts.set_credentials(u.clone(), p.clone());
    }
    if config.tls {
        let path = config.ca_cert_path.as_deref().ok_or_else(|| {
            MqttError::Config(
                "MQTT_TLS is set but MQTT_CA_CERT is not; point it at the broker CA (PEM) or a system bundle such as /etc/ssl/certs/ca-certificates.crt".into(),
            )
        })?;
        // rumqttc builds its rustls config from the process-wide provider; ring is the only
        // one compiled in. Err means one is installed already, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let ca = std::fs::read(path)
            .map_err(|e| MqttError::Config(format!("cannot read MQTT_CA_CERT {path}: {e}")))?;
        opts.set_transport(Transport::Tls(TlsConfiguration::Simple {
            ca,
            alpn: None,
            client_auth: None,
        }));
    }
    if let Some(status) = &config.status {
        opts.set_last_will(LastWill::new(
            status.topic.clone(),
            status.offline.clone(),
            QoS::AtLeastOnce,
            true,
        ));
    }
    // Reasonable channel capacity for requests
    opts.set_request_channel_capacity(64);
    Ok(AsyncClient::new(opts, 64))
}

async fn run_eventloop(
    mut eventloop: EventLoop,
    client_shared: Arc<Mutex<AsyncClient>>,
    ready: Arc<AtomicBool>,
    events_tx: broadcast::Sender<MqttEvent>,
    subscriptions: Arc<RwLock<HashMap<String, QoS>>>,
    config: MqttConfig,
) {
    let mut backoff_secs = 1u64;
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                info!("MQTT connected");
                ready.store(true, Ordering::Relaxed);
                let _ = events_tx.send(MqttEvent::Connected);

                // Restore all tracked subscriptions after reconnection
                let subs = subscriptions.read().await;
                let client = client_shared.lock().await;
                for (topic, qos) in subs.iter() {
                    debug!("Restoring subscription to {}", topic);
                    if let Err(err) = client.subscribe(topic, *qos).await {
                        warn!(?err, "Failed to restore subscription to {}", topic);
                    }
                }
                // Announce presence (retained) so the last will has something to clear
                if let Some(status) = &config.status
                    && let Err(err) = client
                        .publish(&status.topic, QoS::AtLeastOnce, true, status.online.clone())
                        .await
                    {
                        warn!(?err, "Failed to publish server status");
                    }
                drop(client); // Release the client lock
                drop(subs); // Release the read lock

                // Reset backoff on successful connect
                backoff_secs = 1;
            }
            Ok(Event::Incoming(Incoming::Publish(p))) => {
                let topic = p.topic.clone();
                let payload = p.payload.to_vec();
                let _ = events_tx.send(MqttEvent::Publish { topic, payload });
            }
            Ok(Event::Incoming(Incoming::PubAck(ack))) => {
                let _ = events_tx.send(MqttEvent::PubAck(ack.pkid));
            }
            Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                warn!("MQTT disconnect requested");
                ready.store(false, Ordering::Relaxed);
                let _ = events_tx.send(MqttEvent::Disconnected);
            }
            Ok(other) => {
                debug!(?other, "MQTT event");
            }
            Err(e) => {
                error!(error = ?e, "MQTT error; will attempt reconnect");
                ready.store(false, Ordering::Relaxed);
                let _ = events_tx.send(MqttEvent::Disconnected);

                // Exponential backoff with cap
                let wait = backoff_secs.min(30);
                sleep(Duration::from_secs(wait)).await;
                backoff_secs = (backoff_secs * 2).min(60);

                // Attempt to rebuild client and eventloop
                match build_client(&config) {
                    Ok((new_client, new_eventloop)) => {
                        // Replace both eventloop and client with fresh instances
                        eventloop = new_eventloop;
                        {
                            let mut client_guard = client_shared.lock().await;
                            *client_guard = new_client;
                        }
                        info!("MQTT client and eventloop rebuilt, attempting reconnection");
                        // Next poll should connect
                    }
                    Err(err) => {
                        error!(?err, "Failed to rebuild MQTT client; retrying");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_filters() {
        assert!(topic_matches("roaster/#", "roaster/a/telemetry"));
        assert!(topic_matches("roaster/#", "roaster"));
        assert!(topic_matches("roaster/+/telemetry", "roaster/a/telemetry"));
        assert!(!topic_matches("roaster/+/telemetry", "roaster/a/status"));
        assert!(!topic_matches("roaster/+", "roaster/a/telemetry"));
        assert!(topic_matches("a/b", "a/b"));
        assert!(!topic_matches("a/b", "a/b/c"));
        assert!(!topic_matches("a/b/c", "a/b"));
    }

    #[tokio::test]
    async fn loopback_delivers_to_subscribers_and_acks() {
        let mqtt = MqttService::loopback();
        let mut rx = mqtt.events();
        mqtt.publish("roaster/a/telemetry", QoS::AtMostOnce, false, "x")
            .await
            .unwrap();
        assert!(rx.try_recv().is_err(), "nothing is subscribed yet");
        mqtt.subscribe("roaster/#", QoS::AtMostOnce).await.unwrap();
        mqtt.publish("roaster/a/control/fan", QoS::AtLeastOnce, false, "50")
            .await
            .unwrap();
        match rx.try_recv().unwrap() {
            MqttEvent::Publish { topic, payload } => {
                assert_eq!(topic, "roaster/a/control/fan");
                assert_eq!(payload, b"50");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(rx.try_recv().unwrap(), MqttEvent::PubAck(_)));
        assert!(mqtt.is_ready());
    }
}
