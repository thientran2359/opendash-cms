use super::*;

fn opt(value: &str, label: &str) -> SelectOption {
    SelectOption {
        value: value.to_owned(),
        label: label.to_owned(),
        icon: None,
    }
}

fn rule(field: &str, op: ConditionOp, value: Option<Value>) -> ConditionRule {
    ConditionRule {
        field: field.to_owned(),
        op,
        value,
    }
}

mod suite_01;
mod suite_02;
mod suite_03;
mod suite_04;
