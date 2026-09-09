//! Shared grammar for prevention amounts that are relative to an in-flight
//! damage event. These forms must never fall through to a one-damage shield.

use std::num::NonZeroU32;

use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::combinator::{map, map_opt, rest, value};
use nom::sequence::{preceded, terminated};
use nom::Parser;

use crate::parser::oracle_nom::error::OracleResult;
use crate::parser::oracle_nom::primitives::parse_number;
use crate::types::ability::{PreventionFormula, RoundingMode};

/// CR 615.1a + CR 107.1a: parse the amount after `prevent ` when it is a
/// portion of the same damage event. The quantity-binding form deliberately
/// requires its `where X is` tail; an unbound X is not a zero or one fallback.
pub fn parse_damage_prevention_formula(input: &str) -> OracleResult<'_, PreventionFormula> {
    alt((
        map(
            terminated(parse_number, tag(" of that damage")),
            PreventionFormula::fixed,
        ),
        map(
            terminated(parse_number, tag(" damage that")),
            PreventionFormula::fixed,
        ),
        value(
            PreventionFormula::Fraction {
                numerator: 1,
                denominator: NonZeroU32::new(2).expect("2 is nonzero"),
                rounding: RoundingMode::Up,
            },
            tag("half that damage, rounded up"),
        ),
        value(
            PreventionFormula::Fraction {
                numerator: 1,
                denominator: NonZeroU32::new(2).expect("2 is nonzero"),
                rounding: RoundingMode::Down,
            },
            tag("half that damage, rounded down"),
        ),
        map_opt(
            preceded(tag("x of that damage, where x is "), rest),
            |quantity| {
                crate::parser::oracle_quantity::parse_cda_quantity(quantity)
                    .map(|quantity| PreventionFormula::Quantity { quantity })
            },
        ),
    ))
    .parse(input)
}

/// Classify a prevention clause that names an event-relative amount, including
/// unsupported unbound forms. Consumers use this to produce an honest parser
/// gap instead of `PreventDamage { amount: Next(1), .. }`.
pub fn has_event_relative_prevention_amount(input: &str) -> bool {
    // These are parser inputs already normalized to lowercase. `tag` keeps the
    // recognition at a word boundary rather than treating an arbitrary substring
    // as Oracle grammar.
    crate::parser::oracle_nom::primitives::scan_at_word_boundaries(input, |candidate| {
        alt((
            tag::<_, _, crate::parser::oracle_nom::error::OracleError<'_>>("x of that damage"),
            tag("half that damage"),
        ))
        .parse(candidate)
    })
    .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixed_fractional_and_bound_quantity_forms() {
        assert!(matches!(
            parse_damage_prevention_formula("3 of that damage")
                .unwrap()
                .1,
            PreventionFormula::Fixed(3)
        ));
        assert!(matches!(
            parse_damage_prevention_formula("half that damage, rounded up")
                .unwrap()
                .1,
            PreventionFormula::Fraction {
                rounding: RoundingMode::Up,
                ..
            }
        ));
        assert!(matches!(
            parse_damage_prevention_formula(
                "x of that damage, where x is the number of Clerics you control"
            )
            .unwrap()
            .1,
            PreventionFormula::Quantity { .. }
        ));
    }
}
