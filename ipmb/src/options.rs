use crate::Label;

/// Parameters for joining the bus.
#[derive(Debug, Clone)]
pub struct Options {
    /// Identifier for the bus, e.g. `com.myapp`.
    pub identifier: String,
    /// The label of the endpoint through which messages can be routed to the endpoint..
    pub label: Label,
    /// Secturity token.
    pub token: String,
    /// Whether the endpoint can become a bus controller.
    pub controller_affinity: bool,
}

impl Options {
    pub fn new<I, K>(identifier: I, label: Label, token: K) -> Self
    where
        I: Into<String>,
        K: Into<String>,
    {
        Self {
            identifier: identifier.into(),
            label,
            token: token.into(),
            controller_affinity: true,
        }
    }
}
