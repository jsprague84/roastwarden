//! What green coffee cost (`docs/green-coffee.md`, "Costs").
//!
//! A bean's cost per kg is the moving weighted average of what is left: each priced purchase
//! mixes its cost into the stock held at that moment, and every draw (a roast, a loss, a
//! correction down) takes beans out at the average of the day, so it leaves the average as
//! it was. That is what a home roaster's question "what does a kilo in the bag cost me now"
//! means, and it needs no lot-by-lot bookkeeping (FIFO would, and moves the number only by
//! which bag a roast is said to come from).
//!
//! Costs are in the home currency (Settings → Roasting, `currency`). A purchase in another
//! currency, or one with no price, adds its grams at the current average; it is left out of
//! the average rather than mixed in at a made-up rate.

use std::collections::HashMap;

/// One ledger move, as the walk needs it.
#[derive(Debug, Clone)]
pub struct MoveIn {
    pub id: String,
    pub delta_g: f64,
    pub kind: String,
    pub price: Option<f64>,
    pub shipping: Option<f64>,
    pub fees: Option<f64>,
    pub currency: Option<String>,
}

impl MoveIn {
    /// What a purchase cost in `home` currency (coffee, shipping and fees), or `None` when it
    /// has no price or is in another currency.
    pub fn purchase_cost(&self, home: &str) -> Option<f64> {
        if self.kind != "purchase" || self.delta_g <= 0.0 {
            return None;
        }
        let price = self.price?;
        if !self
            .currency
            .as_deref()
            .unwrap_or(home)
            .eq_ignore_ascii_case(home)
        {
            return None;
        }
        Some(price + self.shipping.unwrap_or(0.0) + self.fees.unwrap_or(0.0))
    }
}

/// What one move cost or was worth.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveCost {
    /// A priced purchase: what it cost. A draw: what the beans taken out were worth at the
    /// average of that moment. `None` before any priced purchase.
    pub cost: Option<f64>,
    /// The bean's cost per kg after this move.
    pub avg_per_kg: Option<f64>,
}

/// A bean's ledger, walked.
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    pub stock_g: f64,
    /// Cost per kg of what is left.
    pub avg_per_kg: Option<f64>,
    pub moves: HashMap<String, MoveCost>,
}

impl Ledger {
    /// What is left is worth, at the average (none when nothing is left).
    pub fn stock_value(&self) -> Option<f64> {
        self.avg_per_kg
            .map(|a| a * self.stock_g.max(0.0) / 1000.0)
            .filter(|_| self.stock_g > 0.0)
    }
}

/// Walk one bean's moves, oldest first.
pub fn walk(moves: &[MoveIn], home: &str) -> Ledger {
    let mut l = Ledger::default();
    for m in moves {
        let mut cost = None;
        if let Some(c) = m.purchase_cost(home) {
            let held = l.stock_g.max(0.0);
            // Beans held at no known cost are taken to have cost what this purchase did.
            let avg = match l.avg_per_kg {
                Some(a) if held > 0.0 => (held / 1000.0 * a + c) / ((held + m.delta_g) / 1000.0),
                _ => c / (m.delta_g / 1000.0),
            };
            l.avg_per_kg = Some(avg);
            cost = Some(c);
        } else if m.delta_g < 0.0 {
            cost = l.avg_per_kg.map(|a| -m.delta_g / 1000.0 * a);
        }
        l.stock_g += m.delta_g;
        l.moves.insert(
            m.id.clone(),
            MoveCost {
                cost,
                avg_per_kg: l.avg_per_kg,
            },
        );
    }
    l
}

/// Round money to cents for the API (sums are exact enough at that point).
pub fn cents(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// What a roast's coffee cost: green, per kg roasted, per dose and per bag.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct RoastCost {
    pub currency: String,
    /// The green coffee the roast used, at the bean's average of that day
    pub green_cost: f64,
    pub green_per_kg: f64,
    /// Green cost ÷ roasted weight (green per kg ÷ (1 − loss)); absent until a roasted weight
    pub roasted_per_kg: Option<f64>,
    pub per_espresso_dose: Option<f64>,
    pub per_filter_dose: Option<f64>,
    pub per_bag: Option<f64>,
    pub espresso_dose_g: f64,
    pub filter_dose_g: f64,
    pub bag_g: f64,
}

/// Dose and bag sizes (Settings → Roasting).
#[derive(Debug, Clone, Copy)]
pub struct Sizes {
    pub espresso_g: f64,
    pub filter_g: f64,
    pub bag_g: f64,
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            espresso_g: 18.0,
            filter_g: 15.0,
            bag_g: 250.0,
        }
    }
}

pub fn roast_cost(
    green_cost: f64,
    green_g: f64,
    roasted_g: Option<f64>,
    currency: &str,
    sizes: Sizes,
) -> Option<RoastCost> {
    if green_g <= 0.0 {
        return None;
    }
    let roasted_per_kg = roasted_g
        .filter(|r| *r > 0.0 && *r <= green_g)
        .map(|r| green_cost / (r / 1000.0));
    let per = |g: f64| roasted_per_kg.map(|k| cents(k * g / 1000.0));
    Some(RoastCost {
        currency: currency.to_string(),
        green_cost: cents(green_cost),
        green_per_kg: cents(green_cost / (green_g / 1000.0)),
        roasted_per_kg: roasted_per_kg.map(cents),
        per_espresso_dose: per(sizes.espresso_g),
        per_filter_dose: per(sizes.filter_g),
        per_bag: per(sizes.bag_g),
        espresso_dose_g: sizes.espresso_g,
        filter_dose_g: sizes.filter_g,
        bag_g: sizes.bag_g,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(id: &str, delta_g: f64, kind: &str, price: Option<f64>) -> MoveIn {
        MoveIn {
            id: id.into(),
            delta_g,
            kind: kind.into(),
            price,
            shipping: None,
            fees: None,
            currency: None,
        }
    }

    fn close(a: Option<f64>, b: f64) {
        let a = a.expect("a value");
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    /// 2 kg at 20/kg, 1 kg roasted, then 1 kg at 32/kg plus 4 shipping: the second purchase
    /// mixes with the 1 kg left, (20 + 36) / 2 = 28/kg.
    #[test]
    fn the_average_mixes_purchases_into_what_is_left() {
        let mut p2 = mv("p2", 1000.0, "purchase", Some(32.0));
        p2.shipping = Some(4.0);
        let moves = [
            mv("p1", 2000.0, "purchase", Some(40.0)),
            mv("r1", -1000.0, "roast", None),
            p2,
            mv("r2", -100.0, "roast", None),
        ];
        let l = walk(&moves, "USD");
        close(l.moves["p1"].avg_per_kg, 20.0);
        close(l.moves["r1"].cost, 20.0);
        close(l.moves["p2"].cost, 36.0);
        close(l.avg_per_kg, 28.0);
        close(l.moves["r2"].cost, 2.8);
        assert_eq!(l.stock_g, 1900.0);
        close(l.stock_value(), 53.2);
    }

    #[test]
    fn unpriced_and_foreign_purchases_keep_the_average() {
        let mut eur = mv("e", 1000.0, "purchase", Some(5.0));
        eur.currency = Some("EUR".into());
        let moves = [
            mv("free", 500.0, "purchase", None),
            mv("r0", -100.0, "roast", None),
            mv("p", 1000.0, "purchase", Some(30.0)),
            eur,
            mv("up", 200.0, "adjust", None),
            mv("down", -400.0, "adjust", None),
        ];
        let l = walk(&moves, "usd");
        // Nothing known before the first priced purchase.
        assert_eq!(l.moves["r0"].cost, None);
        // 400 g held at no known cost count at the new price: 30/kg.
        close(l.moves["p"].avg_per_kg, 30.0);
        assert_eq!(l.moves["e"].cost, None);
        close(l.moves["e"].avg_per_kg, 30.0);
        close(l.moves["down"].cost, 12.0);
        close(l.avg_per_kg, 30.0);
    }

    #[test]
    fn a_purchase_after_running_out_starts_a_new_average() {
        let moves = [
            mv("p1", 1000.0, "purchase", Some(20.0)),
            mv("r1", -1100.0, "roast", None),
            mv("p2", 1000.0, "purchase", Some(30.0)),
        ];
        let l = walk(&moves, "USD");
        close(l.moves["r1"].cost, 22.0);
        close(l.avg_per_kg, 30.0);
        assert_eq!(l.stock_g, 900.0);
    }

    #[test]
    fn a_roast_costs_per_kg_roasted_dose_and_bag() {
        // 100 g green at 28/kg = 2.80; 84 g roasted (16 % loss): 33.33/kg.
        let c = roast_cost(2.8, 100.0, Some(84.0), "USD", Sizes::default()).unwrap();
        assert_eq!(c.green_cost, 2.8);
        assert_eq!(c.green_per_kg, 28.0);
        assert_eq!(c.roasted_per_kg, Some(33.33));
        assert_eq!(c.per_espresso_dose, Some(0.6));
        assert_eq!(c.per_filter_dose, Some(0.5));
        assert_eq!(c.per_bag, Some(8.33));
        let none = roast_cost(2.8, 100.0, None, "USD", Sizes::default()).unwrap();
        assert_eq!(none.roasted_per_kg, None);
        assert!(roast_cost(1.0, 0.0, None, "USD", Sizes::default()).is_none());
    }
}
