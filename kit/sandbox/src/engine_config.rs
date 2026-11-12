// How every engine of the sandbox is configured, said once: the runtime (`sandbox::engine`) and the
// build that compiles the interpreter for it (`build.rs`, which includes this file) both take it, so
// that what the build compiled is what an engine loads.

/// Compiled to be interrupted by the epoch, speaking the component model and the exceptions of the
/// interpreters.
pub(crate) fn configure(config: &mut wasmtime::Config) {
    config.epoch_interruption(true);
    config.wasm_component_model(true);
    config.wasm_exceptions(true);
}
