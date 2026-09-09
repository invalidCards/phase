use std::sync::Arc;

use crate::types::ability::{ContinuousModification, StaticDefinition};

/// The copiable characteristic axes an "except" clause supplies while copying.
///
/// CR 707.9d excludes a copied characteristic-defining ability when a copy
/// exception does not copy, retains, or supplies values for that characteristic.
/// These axes deliberately describe only the characteristics this engine's
/// typed copy-exception vocabulary can override.
#[derive(Clone, Copy, Default)]
struct CopyExceptionOverrides {
    card_types: bool,
    color: bool,
    power: bool,
    toughness: bool,
}

impl CopyExceptionOverrides {
    fn from_modifications(modifications: &[ContinuousModification]) -> Self {
        let mut overrides = Self::default();
        for modification in modifications {
            match modification {
                ContinuousModification::SetCardTypes { .. }
                | ContinuousModification::RemoveAllSubtypes { .. } => overrides.card_types = true,
                // CR 707.9d applies when an exception adds a color in addition
                // to the copied object's other colors, too (Lazotep Convert).
                ContinuousModification::AddColor { .. }
                | ContinuousModification::SetColor { .. } => overrides.color = true,
                ContinuousModification::SetPower { .. } => overrides.power = true,
                ContinuousModification::SetToughness { .. } => overrides.toughness = true,
                _ => {}
            }
        }
        overrides
    }

    fn is_empty(self) -> bool {
        !self.card_types && !self.color && !self.power && !self.toughness
    }
}

/// Prunes source CDAs whose defined characteristic is overridden by a copy
/// exception, returning `None` for a CDA shape outside the typed vocabulary.
///
/// CR 707.9d: this is shared by permanent-copy snapshots and copy-token body
/// materialization, because both paths establish the copied object's copiable
/// values. An unclassified CDA leaves its source definitions intact rather than
/// risking an over-broad deletion.
pub(crate) fn prune_copy_exception_overridden_cdas(
    definitions: &Arc<Vec<StaticDefinition>>,
    modifications: &[ContinuousModification],
) -> Option<Vec<StaticDefinition>> {
    let overrides = CopyExceptionOverrides::from_modifications(modifications);
    if overrides.is_empty() {
        return Some(definitions.as_ref().clone());
    }

    let mut retained = Vec::with_capacity(definitions.len());
    for definition in definitions.iter() {
        if !definition.characteristic_defining {
            retained.push(definition.clone());
            continue;
        }

        let axes = cda_defined_axes(definition)?;
        let overridden = (axes.card_types && overrides.card_types)
            || (axes.color && overrides.color)
            || (axes.power && overrides.power)
            || (axes.toughness && overrides.toughness);
        if !overridden {
            retained.push(definition.clone());
        }
    }
    Some(retained)
}

/// A CDA definition is removable as a whole only when each of its modifications
/// has a known characteristic axis. A fixed P/T pair defines both axes.
fn cda_defined_axes(definition: &StaticDefinition) -> Option<CopyExceptionOverrides> {
    let mut axes = CopyExceptionOverrides::default();
    for modification in &definition.modifications {
        match modification {
            ContinuousModification::AddAllCreatureTypes => axes.card_types = true,
            ContinuousModification::SetDynamicPower { .. }
            | ContinuousModification::SetPower { .. } => axes.power = true,
            ContinuousModification::SetDynamicToughness { .. }
            | ContinuousModification::SetToughness { .. } => axes.toughness = true,
            ContinuousModification::SetColor { .. } => axes.color = true,
            _ => return None,
        }
    }
    Some(axes)
}
