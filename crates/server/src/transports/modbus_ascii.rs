//! A Modbus ASCII master (Modbus over Serial Line v1.02, §2.5.2) as a tokio-modbus `Context`,
//! for the PID controllers that speak ASCII on a serial line (Delta DTA/DTK and others in
//! Artisan's machine catalogue). tokio-modbus has RTU and TCP only.
//!
//! A frame is `:`, the slave address and the PDU as uppercase hex, the LRC as hex, then CRLF.
//! The poller's calls (`read_holding_registers`, `write_single_register`, `set_slave`, …) work
//! on the returned `Context` as they do on the RTU one.

use std::fmt::Write as _;
use std::io;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_modbus::client::{Client, Context};
use tokio_modbus::slave::{Slave, SlaveContext};
use tokio_modbus::{ExceptionCode, Request, Response};

/// The longest frame the spec allows: `:`, 255 bytes as hex, CRLF (513 characters).
const MAX_FRAME: usize = 513;

/// A Modbus ASCII master on a serial stream (any `AsyncRead` + `AsyncWrite`), as a tokio-modbus Context.
pub fn attach_slave<T>(transport: T, slave: Slave) -> Context
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let client = AsciiClient {
        transport: Some(transport),
        slave: slave.0,
        buf: Vec::new(),
    };
    Context::from(Box::new(client) as Box<dyn Client>)
}

struct AsciiClient<T> {
    transport: Option<T>,
    slave: u8,
    /// Bytes read but not yet part of a whole frame.
    buf: Vec<u8>,
}

impl<T> SlaveContext for AsciiClient<T> {
    fn set_slave(&mut self, slave: Slave) {
        self.slave = slave.0;
    }
}

#[async_trait]
impl<T> Client for AsciiClient<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
{
    async fn call(&mut self, request: Request<'_>) -> tokio_modbus::Result<Response> {
        let slave = self.slave;
        if slave == 0 {
            return Err(invalid_input("Modbus ASCII: broadcast (slave 0) is not supported").into());
        }
        let pdu = encode_request(&request)?;
        let Some(transport) = self.transport.as_mut() else {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "disconnected").into());
        };
        self.buf.clear();
        transport.write_all(&encode_frame(slave, &pdu)).await?;
        transport.flush().await?;
        let adu = loop {
            if let Some(hex) = take_frame(&mut self.buf)? {
                break decode_adu(&hex)?;
            }
            let mut chunk = [0u8; 256];
            let n = transport.read(&mut chunk).await?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Modbus ASCII: the line closed before an answer",
                )
                .into());
            }
            self.buf.extend_from_slice(&chunk[..n]);
        };
        Ok(decode_response(&request, slave, &adu)?)
    }

    async fn disconnect(&mut self) -> io::Result<()> {
        let Some(mut transport) = self.transport.take() else {
            return Ok(());
        };
        match transport.shutdown().await {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotConnected | io::ErrorKind::BrokenPipe
                ) =>
            {
                Ok(())
            }
            other => other,
        }
    }
}

fn invalid_data(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn invalid_input(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}

/// The LRC: the two's complement of the 8-bit sum of the bytes.
pub(crate) fn lrc(bytes: &[u8]) -> u8 {
    0u8.wrapping_sub(bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)))
}

/// A whole ASCII frame for the slave and PDU: `:`, hex, LRC, CRLF.
pub(crate) fn encode_frame(slave: u8, pdu: &[u8]) -> Vec<u8> {
    let mut adu = Vec::with_capacity(pdu.len() + 2);
    adu.push(slave);
    adu.extend_from_slice(pdu);
    adu.push(lrc(&adu));
    let mut out = String::with_capacity(adu.len() * 2 + 3);
    out.push(':');
    for b in &adu {
        let _ = write!(out, "{b:02X}");
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// The request's PDU (function code and data).
pub(crate) fn encode_request(request: &Request<'_>) -> io::Result<Vec<u8>> {
    let head = |fc: u8, a: u16, b: u16| {
        let mut pdu = vec![fc];
        pdu.extend_from_slice(&a.to_be_bytes());
        pdu.extend_from_slice(&b.to_be_bytes());
        pdu
    };
    Ok(match request {
        Request::ReadCoils(a, q) => head(0x01, *a, *q),
        Request::ReadDiscreteInputs(a, q) => head(0x02, *a, *q),
        Request::ReadHoldingRegisters(a, q) => head(0x03, *a, *q),
        Request::ReadInputRegisters(a, q) => head(0x04, *a, *q),
        Request::WriteSingleCoil(a, on) => head(0x05, *a, if *on { 0xFF00 } else { 0 }),
        Request::WriteSingleRegister(a, w) => head(0x06, *a, *w),
        Request::WriteMultipleCoils(a, coils) => {
            let mut packed = vec![0u8; coils.len().div_ceil(8)];
            for (i, on) in coils.iter().enumerate() {
                if *on {
                    packed[i / 8] |= 1 << (i % 8);
                }
            }
            let mut pdu = head(0x0F, *a, quantity(coils.len())?);
            pdu.push(byte_count(packed.len())?);
            pdu.extend_from_slice(&packed);
            pdu
        }
        Request::WriteMultipleRegisters(a, words) => {
            let mut pdu = head(0x10, *a, quantity(words.len())?);
            pdu.push(byte_count(words.len() * 2)?);
            for w in words.iter() {
                pdu.extend_from_slice(&w.to_be_bytes());
            }
            pdu
        }
        other => return Err(unsupported(other)),
    })
}

fn unsupported(request: &Request<'_>) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "Modbus ASCII: function {} is not supported",
            request.function_code()
        ),
    )
}

fn quantity(n: usize) -> io::Result<u16> {
    u16::try_from(n).map_err(|_| invalid_input("Modbus ASCII: too many values in one request"))
}

fn byte_count(n: usize) -> io::Result<u8> {
    u8::try_from(n).map_err(|_| invalid_input("Modbus ASCII: too many values in one request"))
}

/// Takes the first whole frame out of `buf` and returns its hex characters (between `:` and
/// CRLF); `None` until one has arrived. Bytes before the `:` are noise and dropped; a second
/// `:` before the CRLF starts the frame again.
pub(crate) fn take_frame(buf: &mut Vec<u8>) -> io::Result<Option<Vec<u8>>> {
    loop {
        let Some(start) = buf.iter().position(|b| *b == b':') else {
            buf.clear();
            return Ok(None);
        };
        buf.drain(..start);
        let end = find_crlf(buf);
        let restart = buf[1..].iter().position(|b| *b == b':').map(|i| i + 1);
        match (end, restart) {
            (_, Some(c)) if end.is_none_or(|e| c < e) => {
                buf.drain(..c);
            }
            (Some(e), _) => {
                let hex = buf[1..e].to_vec();
                buf.drain(..e + 2);
                return Ok(Some(hex));
            }
            (None, _) if buf.len() > MAX_FRAME => {
                buf.clear();
                return Err(invalid_data(
                    "Modbus ASCII: frame longer than 513 characters",
                ));
            }
            (None, _) => return Ok(None),
        }
    }
}

fn find_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|w| w == b"\r\n")
}

/// The frame's hex as bytes (slave, PDU), its LRC checked and removed.
pub(crate) fn decode_adu(hex: &[u8]) -> io::Result<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return Err(invalid_data("Modbus ASCII: odd number of hex characters"));
    }
    let digit = |c: u8| {
        (c as char)
            .to_digit(16)
            .ok_or_else(|| invalid_data(format!("Modbus ASCII: {:?} is not hex", c as char)))
    };
    let mut bytes = hex
        .chunks(2)
        .map(|p| Ok((digit(p[0])? * 16 + digit(p[1])?) as u8))
        .collect::<io::Result<Vec<u8>>>()?;
    if bytes.len() < 3 {
        return Err(invalid_data("Modbus ASCII: frame too short"));
    }
    let got = bytes.pop().unwrap_or_default();
    let want = lrc(&bytes);
    if got != want {
        return Err(invalid_data(format!(
            "Modbus ASCII: bad LRC {got:02X}, expected {want:02X}"
        )));
    }
    Ok(bytes)
}

/// The answer to `request` from the decoded frame (slave, PDU): its slave and function checked,
/// an exception answer as `Err(ExceptionCode)`.
pub(crate) fn decode_response(
    request: &Request<'_>,
    slave: u8,
    adu: &[u8],
) -> io::Result<Result<Response, ExceptionCode>> {
    let (&got_slave, pdu) = adu
        .split_first()
        .ok_or_else(|| invalid_data("Modbus ASCII: empty frame"))?;
    if got_slave != slave {
        return Err(invalid_data(format!(
            "Modbus ASCII: answer from slave {got_slave}, asked slave {slave}"
        )));
    }
    let want_fc = request.function_code().value();
    let (&fc, data) = pdu
        .split_first()
        .ok_or_else(|| invalid_data("Modbus ASCII: no function code"))?;
    if fc == want_fc | 0x80 {
        return match data {
            [code] => Ok(Err(ExceptionCode::new(*code))),
            _ => Err(invalid_data("Modbus ASCII: malformed exception answer")),
        };
    }
    if fc != want_fc {
        return Err(invalid_data(format!(
            "Modbus ASCII: answer has function {fc:#04X}, asked {want_fc:#04X}"
        )));
    }
    let short = || invalid_data(format!("Modbus ASCII: short answer to function {fc:#04X}"));
    let word = |i: usize| -> io::Result<u16> {
        data.get(i..i + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .ok_or_else(short)
    };
    // A read's data: its byte count, then exactly that many bytes.
    let counted = |want: usize| -> io::Result<&[u8]> {
        let (&n, rest) = data.split_first().ok_or_else(short)?;
        if usize::from(n) != want || rest.len() != want {
            return Err(invalid_data(format!(
                "Modbus ASCII: answer carries {} bytes (count {n}), expected {want}",
                rest.len()
            )));
        }
        Ok(rest)
    };
    // Coils come back as whole bytes, as tokio-modbus's own codec does; Context truncates.
    let bits = |b: &[u8]| -> Vec<bool> {
        (0..b.len() * 8)
            .map(|i| (b[i / 8] >> (i % 8)) & 1 == 1)
            .collect()
    };
    let words = |b: &[u8]| -> Vec<u16> {
        b.chunks(2)
            .map(|w| u16::from_be_bytes([w[0], w[1]]))
            .collect()
    };
    let response = match request {
        Request::ReadCoils(_, q) => {
            Response::ReadCoils(bits(counted(usize::from(*q).div_ceil(8))?))
        }
        Request::ReadDiscreteInputs(_, q) => {
            Response::ReadDiscreteInputs(bits(counted(usize::from(*q).div_ceil(8))?))
        }
        Request::ReadHoldingRegisters(_, q) => {
            Response::ReadHoldingRegisters(words(counted(usize::from(*q) * 2)?))
        }
        Request::ReadInputRegisters(_, q) => {
            Response::ReadInputRegisters(words(counted(usize::from(*q) * 2)?))
        }
        Request::WriteSingleCoil(..) => {
            let on = match word(2)? {
                0xFF00 => true,
                0x0000 => false,
                v => {
                    return Err(invalid_data(format!(
                        "Modbus ASCII: coil value {v:#06X} is neither on nor off"
                    )))
                }
            };
            Response::WriteSingleCoil(word(0)?, on)
        }
        Request::WriteSingleRegister(..) => Response::WriteSingleRegister(word(0)?, word(2)?),
        Request::WriteMultipleCoils(..) => Response::WriteMultipleCoils(word(0)?, word(2)?),
        Request::WriteMultipleRegisters(..) => Response::WriteMultipleRegisters(word(0)?, word(2)?),
        other => return Err(unsupported(other)),
    };
    Ok(Ok(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncBufReadExt, BufReader, DuplexStream};
    use tokio::task::JoinHandle;
    use tokio_modbus::prelude::*;

    #[test]
    fn lrc_and_frame_match_the_spec_example() {
        // 01 + 03 + 10 + 00 + 00 + 01 = 0x15; two's complement 0x100 - 0x15 = 0xEB.
        assert_eq!(lrc(&[0x01, 0x03, 0x10, 0x00, 0x00, 0x01]), 0xEB);
        let pdu = encode_request(&Request::ReadHoldingRegisters(0x1000, 1)).unwrap();
        assert_eq!(encode_frame(1, &pdu), b":010310000001EB\r\n");
        // A sum that wraps: F7 + 03 + 13 + 89 + 00 + 0A = 0x1A0 → 0xA0 → LRC 0x100 - 0xA0 = 0x60.
        assert_eq!(lrc(&[0xF7, 0x03, 0x13, 0x89, 0x00, 0x0A]), 0x60);
    }

    #[test]
    fn frames_are_found_in_noise_and_lowercase_decodes() {
        let mut buf = b"\x00junk:01030200".to_vec();
        assert!(take_frame(&mut buf).unwrap().is_none());
        buf.extend_from_slice(b"2ad0\r\n:01");
        let hex = take_frame(&mut buf).unwrap().unwrap();
        assert_eq!(hex, b"010302002ad0");
        assert_eq!(buf, b":01");
        // 01 + 03 + 02 + 00 + 2A = 0x30 → LRC 0xD0.
        assert_eq!(decode_adu(&hex).unwrap(), [1, 3, 2, 0, 0x2A]);
        // A restarted frame: the second `:` wins.
        let mut buf = b":0103:010302002AD0\r\n".to_vec();
        assert_eq!(take_frame(&mut buf).unwrap().unwrap(), b"010302002AD0");
        assert!(decode_adu(b"010302002AD1").is_err());
        assert!(decode_adu(b"0103G2").is_err());
    }

    #[test]
    fn unsupported_functions_are_refused() {
        let err = encode_request(&Request::MaskWriteRegister(1, 2, 3)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    /// A fake ASCII slave: reads one frame per answer, checks it is `want`, sends `answer`
    /// (slave and PDU; its LRC added unless `bad_lrc`).
    fn slave(
        mut port: DuplexStream,
        script: Vec<(&'static str, Vec<u8>, bool)>,
    ) -> JoinHandle<DuplexStream> {
        tokio::spawn(async move {
            for (want, answer, bad_lrc) in script {
                let mut line = String::new();
                let mut reader = BufReader::new(&mut port);
                reader.read_line(&mut line).await.unwrap();
                assert_eq!(line, want);
                let mut frame = encode_frame(answer[0], &answer[1..]);
                if bad_lrc {
                    let n = frame.len();
                    frame[n - 3] = if frame[n - 3] == b'0' { b'1' } else { b'0' };
                }
                // Noise before the frame, sent in two pieces.
                port.write_all(b"\0\xff").await.unwrap();
                let (a, b) = frame.split_at(5);
                port.write_all(a).await.unwrap();
                port.write_all(b).await.unwrap();
            }
            port
        })
    }

    fn expect(slave: u8, pdu: &[u8]) -> &'static str {
        String::leak(String::from_utf8(encode_frame(slave, pdu)).unwrap())
    }

    #[tokio::test]
    async fn reads_and_writes_through_the_tokio_modbus_context() {
        let (ours, theirs) = duplex(1024);
        let fake = slave(
            theirs,
            vec![
                (":010310000001EB\r\n", vec![1, 3, 2, 0x01, 0x2C], false),
                (
                    expect(1, &[4, 0, 0x10, 0, 2]),
                    vec![1, 4, 4, 0x12, 0x34, 0xAB, 0xCD],
                    false,
                ),
                (
                    expect(1, &[6, 0x10, 0x01, 0x00, 0xC8]),
                    vec![1, 6, 0x10, 0x01, 0x00, 0xC8],
                    false,
                ),
                (
                    expect(1, &[0x10, 0, 5, 0, 2, 4, 0, 1, 0, 2]),
                    vec![1, 0x10, 0, 5, 0, 2],
                    false,
                ),
                (
                    expect(1, &[1, 0, 0x20, 0, 10]),
                    vec![1, 1, 2, 0b0000_0101, 0b0000_0010],
                    false,
                ),
                (
                    expect(1, &[5, 0x08, 0x13, 0xFF, 0x00]),
                    vec![1, 5, 0x08, 0x13, 0xFF, 0x00],
                    false,
                ),
                (
                    expect(7, &[3, 0x47, 0x00, 0, 1]),
                    vec![7, 3, 2, 0x03, 0xE8],
                    false,
                ),
                (
                    expect(7, &[3, 0x99, 0x99, 0, 1]),
                    vec![7, 0x83, 0x02],
                    false,
                ),
                (expect(7, &[3, 0, 0, 0, 1]), vec![7, 3, 2, 0, 1], true),
                (expect(7, &[3, 0, 0, 0, 1]), vec![8, 3, 2, 0, 1], false),
                (expect(7, &[3, 0, 0, 0, 1]), vec![7, 4, 2, 0, 1], false),
                (expect(7, &[3, 0, 0, 0, 2]), vec![7, 3, 2, 0, 1], false),
            ],
        );
        let mut ctx = attach_slave(ours, Slave(1));

        assert_eq!(
            ctx.read_holding_registers(0x1000, 1)
                .await
                .unwrap()
                .unwrap(),
            [300]
        );
        assert_eq!(
            ctx.read_input_registers(0x10, 2).await.unwrap().unwrap(),
            [0x1234, 0xABCD]
        );
        ctx.write_single_register(0x1001, 200)
            .await
            .unwrap()
            .unwrap();
        ctx.write_multiple_registers(5, &[1, 2])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            ctx.read_coils(0x20, 10).await.unwrap().unwrap(),
            [true, false, true, false, false, false, false, false, false, true]
        );
        ctx.write_single_coil(0x0813, true).await.unwrap().unwrap();

        ctx.set_slave(Slave(7));
        assert_eq!(
            ctx.read_holding_registers(0x4700, 1)
                .await
                .unwrap()
                .unwrap(),
            [1000]
        );
        assert_eq!(
            ctx.read_holding_registers(0x9999, 1).await.unwrap(),
            Err(ExceptionCode::IllegalDataAddress)
        );
        for what in ["bad LRC", "slave 8", "function 0x04", "carries 2 bytes"] {
            let qty = if what == "carries 2 bytes" { 2 } else { 1 };
            let err = ctx.read_holding_registers(0, qty).await.unwrap_err();
            assert!(err.to_string().contains(what), "{what:?} not in {err}");
        }
        fake.await.unwrap();

        ctx.set_slave(Slave(0));
        assert!(ctx.read_holding_registers(0, 1).await.is_err());
        ctx.disconnect().await.unwrap();
    }
}
