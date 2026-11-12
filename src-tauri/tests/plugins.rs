#[path = "plugins/mcp.rs"]
mod mcp;
mod support;
// One harness lets isolated subprocess cases run concurrently after one build.
#[path = "plugins/faults.rs"]
mod faults;
#[path = "plugins/icons.rs"]
mod icons;
#[path = "plugins/performance.rs"]
mod performance;
#[path = "plugins/process.rs"]
mod process;
