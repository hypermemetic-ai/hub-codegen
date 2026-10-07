/// Events from the tickets activation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TicketsEvent {
    /// A ticket was created
    #[serde(rename = "created")]
    Created {
id: String,
title: String,
    },
    /// A ticket's status changed
    #[serde(rename = "status_changed")]
    StatusChanged {
id: String,
previous: String,
    },
    #[serde(rename = "commits_linked")]
    CommitsLinked {
id: String,
added: Vec<String>,
    },
    /// The watch has replayed its backlog
    #[serde(rename = "caught_up")]
    CaughtUp,
    #[serde(rename = "lagged")]
    Lagged {
missed: u64,
    },
}
