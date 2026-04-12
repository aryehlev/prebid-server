//! Skeleton for floors enforcement. Mirrors `floors/enforce.go`.
//!
//! This crate is intentionally decoupled from the real `openrtb` types,
//! so we model bids with a lightweight struct. A downstream crate can
//! build adapters between the real seatbids and these types.

use crate::types::{PriceFloorRules, FLOOR_PRECISION};

/// Minimal bid representation for floors enforcement.
#[derive(Debug, Clone)]
pub struct Bid {
    pub id: String,
    pub imp_id: String,
    pub price: f64,
    pub deal_id: Option<String>,
}

/// Minimal seat bid representation.
#[derive(Debug, Clone, Default)]
pub struct SeatBid {
    pub seat: String,
    pub currency: String,
    pub bids: Vec<Bid>,
}

/// A bid rejected by floors enforcement.
#[derive(Debug, Clone)]
pub struct RejectedBid {
    pub seat: String,
    pub currency: String,
    pub bid: Bid,
}

/// Impression metadata needed for enforcement.
#[derive(Debug, Clone, Default)]
pub struct ImpFloor {
    pub id: String,
    pub bid_floor: f64,
    pub bid_floor_cur: String,
}

/// Outcome of an enforcement pass.
#[derive(Debug, Default)]
pub struct EnforceOutcome {
    /// The surviving seatbids with rejected bids removed.
    pub seatbids: Vec<SeatBid>,
    /// Bids rejected for failing floors enforcement.
    pub rejected: Vec<RejectedBid>,
    /// Non-fatal errors collected during enforcement.
    pub errors: Vec<String>,
}

/// A callable providing a currency conversion rate between two
/// currencies. Returning `None` signals a missing rate.
pub type ConversionFn = dyn Fn(&str, &str) -> Option<f64> + Send + Sync;

/// Enforce floor rules against a set of seat bids.
///
/// This is a skeleton port: it iterates over bids, looks up the
/// matching impression floor, optionally converts currencies and drops
/// bids priced below the floor (respecting deal floors).
pub fn enforce_floors_rules(
    rules: &PriceFloorRules,
    imps: &[ImpFloor],
    seatbids: Vec<SeatBid>,
    enforce_deal_floors: bool,
    convert: &ConversionFn,
) -> EnforceOutcome {
    let mut outcome = EnforceOutcome::default();

    // Early exit: if PBS enforcement is disabled, return bids untouched.
    if !rules.get_enforce_pbs() {
        outcome.seatbids = seatbids;
        return outcome;
    }

    let imp_map: std::collections::HashMap<&str, &ImpFloor> =
        imps.iter().map(|i| (i.id.as_str(), i)).collect();

    for mut seat in seatbids.into_iter() {
        let mut eligible = Vec::with_capacity(seat.bids.len());
        let seat_currency = seat.currency.clone();
        for bid in seat.bids.drain(..) {
            let imp = match imp_map.get(bid.imp_id.as_str()) {
                Some(imp) => imp,
                None => {
                    eligible.push(bid);
                    continue;
                }
            };

            // Deal bids skip enforcement unless enforce_deal_floors is set.
            let is_deal = bid.deal_id.as_ref().map_or(false, |d| !d.is_empty());
            if is_deal && !enforce_deal_floors {
                eligible.push(bid);
                continue;
            }

            let rate = if seat_currency == imp.bid_floor_cur || imp.bid_floor_cur.is_empty() {
                1.0
            } else {
                match convert(&seat_currency, &imp.bid_floor_cur) {
                    Some(r) => r,
                    None => {
                        outcome.errors.push(format!(
                            "rate conversion failed for seat={} from={} to={}",
                            seat.seat, seat_currency, imp.bid_floor_cur
                        ));
                        eligible.push(bid);
                        continue;
                    }
                }
            };

            let effective_price = rate * bid.price;
            if (effective_price + FLOOR_PRECISION) < imp.bid_floor {
                outcome.rejected.push(RejectedBid {
                    seat: seat.seat.clone(),
                    currency: seat_currency.clone(),
                    bid,
                });
                continue;
            }
            eligible.push(bid);
        }
        seat.bids = eligible;
        outcome.seatbids.push(seat);
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_conversion(_from: &str, _to: &str) -> Option<f64> {
        Some(1.0)
    }

    #[test]
    fn enforce_drops_below_floor() {
        let rules = PriceFloorRules::default();
        let imps = vec![ImpFloor {
            id: "imp1".into(),
            bid_floor: 1.0,
            bid_floor_cur: "USD".into(),
        }];
        let seatbids = vec![SeatBid {
            seat: "bidderA".into(),
            currency: "USD".into(),
            bids: vec![
                Bid {
                    id: "b1".into(),
                    imp_id: "imp1".into(),
                    price: 0.5,
                    deal_id: None,
                },
                Bid {
                    id: "b2".into(),
                    imp_id: "imp1".into(),
                    price: 2.0,
                    deal_id: None,
                },
            ],
        }];

        let convert: &ConversionFn = &no_conversion;
        let out = enforce_floors_rules(&rules, &imps, seatbids, false, convert);
        assert_eq!(out.seatbids.len(), 1);
        assert_eq!(out.seatbids[0].bids.len(), 1);
        assert_eq!(out.seatbids[0].bids[0].id, "b2");
        assert_eq!(out.rejected.len(), 1);
        assert_eq!(out.rejected[0].bid.id, "b1");
    }

    #[test]
    fn enforce_skips_deal_bids_by_default() {
        let rules = PriceFloorRules::default();
        let imps = vec![ImpFloor {
            id: "imp1".into(),
            bid_floor: 5.0,
            bid_floor_cur: "USD".into(),
        }];
        let seatbids = vec![SeatBid {
            seat: "bidderA".into(),
            currency: "USD".into(),
            bids: vec![Bid {
                id: "b1".into(),
                imp_id: "imp1".into(),
                price: 0.5,
                deal_id: Some("deal-1".into()),
            }],
        }];

        let convert: &ConversionFn = &no_conversion;
        let out = enforce_floors_rules(&rules, &imps, seatbids, false, convert);
        assert_eq!(out.rejected.len(), 0);
        assert_eq!(out.seatbids[0].bids.len(), 1);
    }
}
