//! Onakke Oathkeeper's graveyard activation must use its printed activation
//! zone and pay both parts of its composite cost through the normal action
//! pipeline.

use engine::game::scenario::{GameScenario, P0, P1};
use engine::game::zones::move_to_zone;
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const ONAKKE_OATHKEEPER_ORACLE: &str = "Creatures can't attack planeswalkers you control unless their controller pays {1} for each creature they control that's attacking a planeswalker you control.\n{4}{W}{W}, Exile this card from your graveyard: Return target planeswalker card from your graveyard to the battlefield.";

fn mana(count: usize, mana_type: ManaType) -> Vec<ManaUnit> {
    (0..count)
        .map(|_| ManaUnit::new(mana_type, ObjectId(0), false, vec![]))
        .collect()
}

/// CR 602.1 + CR 118.12: the printed graveyard activation pays
/// `{4}{W}{W}` and exiles its source, then can return only its controller's
/// planeswalker card from that graveyard. This intentionally uses the normal
/// `GameRunner::activate` action pipeline rather than a spell-cast driver.
#[test]
fn onakke_oathkeeper_graveyard_activation_returns_own_planeswalker() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let oathkeeper = scenario
        .add_creature_from_oracle(P0, "Onakke Oathkeeper", 2, 2, ONAKKE_OATHKEEPER_ORACLE)
        .id();
    let own_planeswalker = scenario
        .add_creature(P0, "Own Jace", 0, 0)
        .as_planeswalker_with_loyalty("Jace", 4)
        .id();
    let opponent_planeswalker = scenario
        .add_creature(P1, "Opponent Chandra", 0, 0)
        .as_planeswalker_with_loyalty("Chandra", 4)
        .id();
    scenario.with_mana_pool(
        P0,
        mana(4, ManaType::Colorless)
            .into_iter()
            .chain(mana(2, ManaType::White))
            .collect(),
    );
    let mut runner = scenario.build();
    let mut events = Vec::new();
    move_to_zone(runner.state_mut(), oathkeeper, Zone::Graveyard, &mut events);
    move_to_zone(
        runner.state_mut(),
        own_planeswalker,
        Zone::Graveyard,
        &mut events,
    );
    move_to_zone(
        runner.state_mut(),
        opponent_planeswalker,
        Zone::Graveyard,
        &mut events,
    );

    // The target intent is intentionally only the controller's planeswalker:
    // the activation's target filter excludes the opponent's graveyard card.
    runner
        .activate(oathkeeper, 0)
        .target_object(own_planeswalker)
        .pay_with(&[oathkeeper])
        .resolve();

    assert_eq!(runner.state().objects[&oathkeeper].zone, Zone::Exile);
    assert_eq!(
        runner.state().objects[&own_planeswalker].zone,
        Zone::Battlefield
    );
    assert_eq!(
        runner.state().objects[&opponent_planeswalker].zone,
        Zone::Graveyard,
        "the opponent's planeswalker was never a legal target"
    );
}
