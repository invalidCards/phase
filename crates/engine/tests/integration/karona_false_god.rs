//! Regression for Karona, False God's phase-triggered control handoff.

use engine::game::scenario::{GameRunner, GameScenario};
use engine::types::actions::GameAction;
use engine::types::game_state::WaitingFor;
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::triggers::TriggerMode;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

const KARONA_ORACLE: &str = "Haste\n\
    At the beginning of each player's upkeep, that player untaps Karona and gains control of it.\n\
    Whenever Karona attacks, creatures of the creature type of your choice get +3/+3 until end of turn.";

/// Drive normal game actions until P1's upkeep trigger has resolved, while
/// recording that the asserted handoff actually occurred during P1's upkeep.
fn advance_until_karona_controlled_by_p1(
    runner: &mut GameRunner,
    karona: engine::types::identifiers::ObjectId,
) -> bool {
    let mut reached_p1_upkeep = false;
    for _ in 0..240 {
        reached_p1_upkeep |=
            runner.state().active_player == P1 && runner.state().phase == Phase::Upkeep;
        if reached_p1_upkeep && runner.state().objects[&karona].controller == P1 {
            return true;
        }
        match &runner.state().waiting_for {
            WaitingFor::Priority { .. } => {
                if runner.act(GameAction::PassPriority).is_err() {
                    return false;
                }
            }
            WaitingFor::DeclareAttackers { .. } => {
                if runner
                    .act(GameAction::DeclareAttackers {
                        attacks: vec![],
                        bands: vec![],
                    })
                    .is_err()
                {
                    return false;
                }
            }
            WaitingFor::DeclareBlockers { .. } => {
                if runner
                    .act(GameAction::DeclareBlockers {
                        assignments: vec![],
                    })
                    .is_err()
                {
                    return false;
                }
            }
            _ => return false,
        }
    }
    false
}

/// CR 608.2c: P1 is the scoped player on P1's upkeep, so Karona's printed
/// untap and control instructions resolve for P1 in order.
#[test]
fn karona_upkeep_untaps_and_transfers_to_the_upkeep_player() {
    let mut scenario = GameScenario::new_n_player(2, 42);
    scenario.at_phase(Phase::PreCombatMain);
    for &player in &[P0, P1] {
        scenario.with_library_top(player, &["Lib A", "Lib B", "Lib C", "Lib D"]);
    }

    let karona = scenario
        .add_creature_from_oracle(P0, "Karona, False God", 5, 5, KARONA_ORACLE)
        .id();
    let mut runner = scenario.build();

    assert!(
        runner.state().objects[&karona]
            .trigger_definitions
            .iter_unchecked()
            .any(|entry| {
                entry.definition.mode == TriggerMode::Phase
                    && entry.definition.phase == Some(Phase::Upkeep)
            }),
        "complete Karona Oracle text must provide its phase/upkeep trigger"
    );
    runner.state_mut().objects.get_mut(&karona).unwrap().tapped = true;

    assert!(
        advance_until_karona_controlled_by_p1(&mut runner, karona),
        "must reach and resolve Karona's trigger during P1's upkeep"
    );
    assert_eq!(
        runner.state().active_player,
        P1,
        "handoff must occur on P1's turn"
    );
    assert_eq!(
        runner.state().phase,
        Phase::Upkeep,
        "handoff must occur during upkeep"
    );
    let object = &runner.state().objects[&karona];
    assert_eq!(object.controller, P1, "P1 must gain control of Karona");
    assert!(
        !object.tapped,
        "Karona's trigger must untap it before the handoff"
    );
}
