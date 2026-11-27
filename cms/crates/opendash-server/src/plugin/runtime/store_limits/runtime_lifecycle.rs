use super::*;

impl PluginRuntime {}

#[cfg(test)]
pub(crate) static TEST_STAGE_WRITE_BARRIER_HIT: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(crate) static TEST_STAGE_WRITE_PAUSE: AtomicBool = AtomicBool::new(false);
