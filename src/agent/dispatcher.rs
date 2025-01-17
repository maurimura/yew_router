//! Dispatcher to RouteAgent.
use crate::agent::{AgentState, RouteAgent};
use std::{
    fmt::{Debug, Error as FmtError, Formatter},
    ops::{Deref, DerefMut},
};
use yew::agent::{Dispatched, Dispatcher};

/// A wrapped dispatcher to the route agent.
///
/// A component that owns and instance of this can send messages to the RouteAgent, but not receive them.
pub struct RouteAgentDispatcher<T>(Dispatcher<RouteAgent<T>>)
where
    for<'de> T: AgentState<'de>;

impl<T> RouteAgentDispatcher<T>
where
    for<'de> T: AgentState<'de>,
{
    /// Creates a new bridge.
    pub fn new() -> Self {
        let dispatcher = RouteAgent::dispatcher();
        RouteAgentDispatcher(dispatcher)
    }
}

impl<T> Default for RouteAgentDispatcher<T>
where
    for<'de> T: AgentState<'de>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T: for<'de> AgentState<'de>> Debug for RouteAgentDispatcher<T> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        f.debug_tuple("RouteAgentDispatcher").finish()
    }
}

impl<T: for<'de> AgentState<'de>> Deref for RouteAgentDispatcher<T> {
    type Target = Dispatcher<RouteAgent<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: for<'de> AgentState<'de>> DerefMut for RouteAgentDispatcher<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
