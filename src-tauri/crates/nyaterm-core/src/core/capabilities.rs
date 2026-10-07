use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskReasonCode {
    EmptyCommand,
    IrreversiblePattern,
    UnclassifiedCommand,
    PrivilegedMutation,
    UnknownCommand,
    OrdinaryWrite,
    ReadOnlyDiagnostic,
}
