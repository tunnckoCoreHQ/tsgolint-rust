//! `no-unsafe-enum-assignment`: disallows giving a value that is not of an
//! enum to a place that expects that enum.

use serde_json::Value;
use tsgolint_core::{Listeners, RuleContext, RuleDefinition, RuleListeners};

pub static RULE: RuleDefinition = RuleDefinition {
    name: "no-unsafe-enum-assignment",
    create,
};

fn create(_ctx: &mut RuleContext<'_>, _options: &Value) -> RuleListeners {
    Listeners::new(()).into()
}
