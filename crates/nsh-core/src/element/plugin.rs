//! Runtime capability instances owned by one VM and described by its registries.

use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use super::{JsError, Local, Registry, System, Tokens, caught};

/// A runtime capability with one instance per VM. Registration stores its
/// definition; initialization runs before the app's entry module.
pub trait Plugin: Default + 'static {
    /// A stable, vendor-qualified identity, independent of JS export names.
    const ID: &'static str;
    /// The service contract version, independent of the crate's version.
    const VERSION: u32 = 1;
    /// Services initialized before this plugin.
    const REQUIRES: &'static [PluginDependency] = &[];

    /// Initializes the instance with the VM's existing capability grants.
    /// Dependencies can be read with [`System::plugin`].
    fn start(&self, _system: &System) -> Result<(), String> {
        Ok(())
    }

    /// Releases subscriptions and pending work. Called once, including
    /// after a failed start, before this plugin's dependencies are stopped.
    /// Keep any dependency handles needed here during [`Self::start`]:
    /// lookups are unavailable once disposal begins.
    fn stop(&self) {}
}

/// A required runtime service and the contract its consumer implements.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct PluginDependency {
    /// The provider's [`Plugin::ID`].
    pub id: &'static str,
    /// The required [`Plugin::VERSION`].
    pub version: u32,
}

impl PluginDependency {
    /// A dependency that can be listed in [`Plugin::REQUIRES`].
    pub const fn new(id: &'static str, version: u32) -> Self {
        Self { id, version }
    }
}

/// A synchronous runtime publication and its queued UI operation. The
/// adapter accepts the operation before publishing tokens, so refusal
/// leaves both the current tokens and the operation queue unchanged.
#[derive(Default)]
#[non_exhaustive]
pub struct RuntimeUpdate {
    /// Work committed with the other operations from this JS turn.
    pub op: Option<crate::Op>,
    /// Tokens visible to `token()` before the calling function returns.
    pub tokens: Option<Arc<Tokens>>,
}

impl RuntimeUpdate {
    /// Queues `op` with this publication.
    pub fn operation(mut self, op: crate::Op) -> Self {
        self.op = Some(op);
        self
    }

    /// Publishes `tokens` in the same call.
    pub fn tokens(mut self, tokens: Arc<Tokens>) -> Self {
        self.tokens = Some(tokens);
        self
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Definition {
    type_id: TypeId,
    id: &'static str,
    version: u32,
    requires: &'static [PluginDependency],
    make: fn() -> Entry,
}

impl Definition {
    pub(crate) fn of<P: Plugin>() -> Self {
        Self {
            type_id: TypeId::of::<P>(),
            id: P::ID,
            version: P::VERSION,
            requires: P::REQUIRES,
            make: || Entry {
                id: P::ID,
                state: Rc::new(P::default()),
                start: |state, system| {
                    state
                        .downcast_ref::<P>()
                        .expect("the registered plugin type")
                        .start(system)
                },
                stop: |state| {
                    state
                        .downcast_ref::<P>()
                        .expect("the registered plugin type")
                        .stop()
                },
            },
        }
    }
}

struct Entry {
    id: &'static str,
    state: Rc<dyn Any>,
    start: fn(&dyn Any, &System) -> Result<(), String>,
    stop: fn(&dyn Any),
}

impl Entry {
    fn stop(&self) {
        if let Err(error) = caught(|| (self.stop)(&*self.state)) {
            log::warn!("plugin {}: stop: {error}", self.id);
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Phase {
    #[default]
    New,
    Starting,
    Active,
    Stopped,
}

#[derive(Default)]
struct Instances {
    phase: Cell<Phase>,
    entries: RefCell<Vec<Entry>>,
}

impl Local for Instances {
    fn close(&self) {
        if self.phase.replace(Phase::Stopped) == Phase::Stopped {
            return;
        }
        self.stop_entries();
    }
}

impl Instances {
    fn stop_entries(&self) {
        loop {
            let entry = self.entries.borrow_mut().pop();
            let Some(entry) = entry else { break };
            entry.stop();
        }
    }

    fn start(&self, system: &System, definitions: &[Definition]) -> Result<(), String> {
        let phase = self.phase.get();
        match phase {
            Phase::Starting => return Err("runtime plugins are already starting".into()),
            Phase::Stopped => return Err("runtime plugins have stopped".into()),
            Phase::New | Phase::Active => {}
        }
        let order = order(definitions)?;
        if phase == Phase::Active {
            let entries = self.entries.borrow();
            if definitions.len() == entries.len()
                && definitions.iter().all(|definition| {
                    entries.iter().any(|entry| {
                        entry.id == definition.id
                            && entry.state.as_ref().type_id() == definition.type_id
                    })
                })
            {
                return Ok(());
            }
            return Err("runtime plugin composition cannot change after startup".into());
        }
        self.phase.set(Phase::Starting);
        for i in order {
            let entry = match caught(|| (definitions[i].make)()) {
                Ok(entry) => entry,
                Err(error) => {
                    self.close();
                    return Err(format!("plugin {}: {error}", definitions[i].id));
                }
            };
            let started = if system.locals.is_closed() || self.phase.get() != Phase::Starting {
                Err("runtime plugins stopped during initialization".into())
            } else {
                caught(|| (entry.start)(&*entry.state, system)).and_then(|r| r)
            };
            let started = started.and_then(|()| {
                if system.locals.is_closed() || self.phase.get() != Phase::Starting {
                    Err("runtime plugins stopped during initialization".into())
                } else {
                    Ok(())
                }
            });
            if let Err(error) = started {
                self.phase.set(Phase::Stopped);
                entry.stop();
                self.stop_entries();
                return Err(format!("plugin {}: {error}", entry.id));
            }
            self.entries.borrow_mut().push(entry);
        }
        self.phase.set(Phase::Active);
        Ok(())
    }
}

fn order(definitions: &[Definition]) -> Result<Vec<usize>, String> {
    for (i, definition) in definitions.iter().enumerate() {
        if definition.id.is_empty() || definition.version == 0 {
            return Err("a runtime plugin needs an ID and a nonzero contract version".into());
        }
        if definitions[..i]
            .iter()
            .any(|other| other.id == definition.id)
        {
            return Err(format!(
                "runtime plugin {} is registered twice",
                definition.id
            ));
        }
    }
    fn visit(
        i: usize,
        definitions: &[Definition],
        states: &mut [u8],
        path: &mut Vec<usize>,
        out: &mut Vec<usize>,
    ) -> Result<(), String> {
        match states[i] {
            2 => return Ok(()),
            1 => {
                let start = path
                    .iter()
                    .position(|&p| p == i)
                    .expect("a visiting dependency");
                let cycle = path[start..]
                    .iter()
                    .copied()
                    .chain(std::iter::once(i))
                    .map(|p| definitions[p].id)
                    .collect::<Vec<_>>()
                    .join(" -> ");
                return Err(format!("runtime plugin dependency cycle: {cycle}"));
            }
            _ => {}
        }
        states[i] = 1;
        path.push(i);
        for dependency in definitions[i].requires {
            let Some(index) = definitions.iter().position(|d| d.id == dependency.id) else {
                return Err(format!(
                    "plugin {} requires {}",
                    definitions[i].id, dependency.id
                ));
            };
            if definitions[index].version != dependency.version {
                return Err(format!(
                    "plugin {} requires {} contract {}, found {}",
                    definitions[i].id,
                    dependency.id,
                    dependency.version,
                    definitions[index].version,
                ));
            }
            visit(index, definitions, states, path, out)?;
        }
        states[i] = 2;
        path.pop();
        out.push(i);
        Ok(())
    }
    let mut states = vec![0; definitions.len()];
    let mut path = Vec::with_capacity(definitions.len());
    let mut out = Vec::with_capacity(definitions.len());
    for i in 0..definitions.len() {
        visit(i, definitions, &mut states, &mut path, &mut out)?;
    }
    Ok(out)
}

impl System {
    /// Starts the plugins of these registries and their submodules in
    /// dependency order. Called by the runtime before evaluating the app;
    /// repeated calls with the same composition keep its instances. A
    /// different composition or a closed VM is refused.
    #[doc(hidden)]
    pub fn start_plugins(&self, registries: &[Registry]) -> Result<(), String> {
        if self.locals.is_closed() {
            return Err("runtime plugins have stopped".into());
        }
        let definitions: Vec<_> = registries
            .iter()
            .flat_map(Registry::sets)
            .flat_map(|r| r.plugins().iter().copied())
            .collect();
        self.locals.get::<Instances>().start(self, &definitions)
    }

    /// The VM's initialized `P`. Callers retain only the capabilities
    /// this VM already has; registration does not grant permissions.
    /// During startup, only instances that have completed initialization
    /// are available. Declare required instances in [`Plugin::REQUIRES`]
    /// to ensure they start first. Lookups fail once disposal begins.
    pub fn plugin<P: Plugin>(&self) -> Result<Rc<P>, JsError> {
        if self.locals.is_closed() {
            return Err(unavailable(P::ID));
        }
        let instances = self.locals.get::<Instances>();
        if !matches!(instances.phase.get(), Phase::Starting | Phase::Active) {
            return Err(unavailable(P::ID));
        }
        let state = instances
            .entries
            .borrow()
            .iter()
            .find(|entry| entry.state.as_ref().type_id() == TypeId::of::<P>())
            .map(|entry| entry.state.clone());
        state
            .and_then(|state| state.downcast().ok())
            .ok_or_else(|| unavailable(P::ID))
    }
}

fn unavailable(id: &str) -> JsError {
    JsError::Error(format!("runtime plugin {id} is unavailable"))
        .with_code(super::codes::PLUGIN_UNAVAILABLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Trace(RefCell<Vec<&'static str>>);

    impl Local for Trace {}

    #[derive(Default)]
    struct Base {
        trace: RefCell<Option<Rc<Trace>>>,
        value: Cell<u32>,
        stopped: Cell<bool>,
    }

    impl Plugin for Base {
        const ID: &'static str = "test.base";

        fn start(&self, system: &System) -> Result<(), String> {
            let trace = system.locals.get::<Trace>();
            trace.0.borrow_mut().push("base.start");
            self.trace.replace(Some(trace));
            self.value.set(7);
            Ok(())
        }

        fn stop(&self) {
            self.stopped.set(true);
            self.trace
                .borrow()
                .as_ref()
                .unwrap()
                .0
                .borrow_mut()
                .push("base.stop");
        }
    }

    #[derive(Default)]
    struct Consumer {
        base: RefCell<Option<Rc<Base>>>,
    }

    impl Plugin for Consumer {
        const ID: &'static str = "test.consumer";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new(Base::ID, 1)];

        fn start(&self, system: &System) -> Result<(), String> {
            let base = system
                .plugin::<Base>()
                .map_err(|e| e.message().to_owned())?;
            assert_eq!(base.value.get(), 7);
            assert!(
                system.plugin::<Self>().is_err(),
                "the current instance is not ready yet"
            );
            assert!(
                system
                    .start_plugins(&[])
                    .unwrap_err()
                    .contains("already starting")
            );
            base.trace
                .borrow()
                .as_ref()
                .unwrap()
                .0
                .borrow_mut()
                .push("consumer.start");
            self.base.replace(Some(base));
            Ok(())
        }

        fn stop(&self) {
            let base = self.base.borrow_mut().take().unwrap();
            assert!(
                !base.stopped.get(),
                "a dependency must outlive its consumer"
            );
            base.trace
                .borrow()
                .as_ref()
                .unwrap()
                .0
                .borrow_mut()
                .push("consumer.stop");
        }
    }

    fn system() -> (System, Rc<Trace>) {
        let system = System::default();
        let trace = system.locals.get::<Trace>();
        (system, trace)
    }

    fn composition() -> Registry {
        let mut registry = Registry::new();
        registry.plugin::<Consumer>().plugin::<Base>();
        registry
    }

    #[test]
    fn dependencies_start_first_and_dispose_once_in_reverse_order() {
        let (system, trace) = system();
        system.start_plugins(&[composition()]).unwrap();
        assert_eq!(&*trace.0.borrow(), &["base.start", "consumer.start"]);
        let base = system.plugin::<Base>().unwrap();
        let consumer = system.plugin::<Consumer>().unwrap();
        assert!(Rc::ptr_eq(&base, consumer.base.borrow().as_ref().unwrap()));
        system.locals.close_all();
        system.locals.close_all();
        assert_eq!(
            &*trace.0.borrow(),
            &["base.start", "consumer.start", "consumer.stop", "base.stop"]
        );
        assert!(base.stopped.get());
        assert_eq!(
            system.plugin::<Base>().err().unwrap().code(),
            Some(super::super::codes::PLUGIN_UNAVAILABLE)
        );
        assert!(
            system
                .start_plugins(&[composition()])
                .unwrap_err()
                .contains("stopped")
        );
    }

    #[test]
    fn repeated_start_keeps_instances_but_refuses_a_changed_composition() {
        let (system, trace) = system();
        system.start_plugins(&[composition()]).unwrap();
        let base = system.plugin::<Base>().unwrap();
        base.value.set(19);
        let mut reordered = Registry::new();
        reordered.plugin::<Base>().plugin::<Consumer>();
        system.start_plugins(&[reordered]).unwrap();
        assert!(Rc::ptr_eq(&base, &system.plugin::<Base>().unwrap()));
        assert_eq!(system.plugin::<Base>().unwrap().value.get(), 19);
        let mut reduced = Registry::new();
        reduced.plugin::<Base>();
        assert!(
            system
                .start_plugins(&[reduced])
                .unwrap_err()
                .contains("cannot change")
        );
        let mut replacement = Registry::new();
        replacement.plugin::<Duplicate>().plugin::<Consumer>();
        assert!(
            system
                .start_plugins(&[replacement])
                .unwrap_err()
                .contains("cannot change")
        );
        assert_eq!(&*trace.0.borrow(), &["base.start", "consumer.start"]);
        system.locals.close_all();
    }

    #[test]
    fn instances_are_isolated_even_when_engines_share_the_registry() {
        let (left, left_trace) = system();
        let (right, right_trace) = system();
        let registry = composition();
        left.start_plugins(std::slice::from_ref(&registry)).unwrap();
        right
            .start_plugins(std::slice::from_ref(&registry))
            .unwrap();
        let left_base = left.plugin::<Base>().unwrap();
        let right_base = right.plugin::<Base>().unwrap();
        assert!(!Rc::ptr_eq(&left_base, &right_base));
        left_base.value.set(42);
        assert_eq!(right_base.value.get(), 7);
        let weak = Rc::downgrade(&left_base);
        left.locals.close_all();
        drop(left_base);
        assert!(
            weak.upgrade().is_none(),
            "disposed dependencies should be released"
        );
        assert!(right.plugin::<Base>().is_ok());
        assert!(!right_base.stopped.get());
        assert_eq!(
            &*left_trace.0.borrow(),
            &["base.start", "consumer.start", "consumer.stop", "base.stop"]
        );
        assert_eq!(&*right_trace.0.borrow(), &["base.start", "consumer.start"]);
        right.locals.close_all();
    }

    #[derive(Default)]
    struct Duplicate;
    impl Plugin for Duplicate {
        const ID: &'static str = Base::ID;
    }

    #[derive(Default)]
    struct Missing;
    impl Plugin for Missing {
        const ID: &'static str = "test.missing-consumer";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new("test.absent", 1)];
    }

    #[derive(Default)]
    struct Incompatible;
    impl Plugin for Incompatible {
        const ID: &'static str = "test.incompatible";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new(Base::ID, 2)];
    }

    #[derive(Default)]
    struct CycleA;
    impl Plugin for CycleA {
        const ID: &'static str = "test.cycle-a";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new(CycleB::ID, 1)];
    }

    #[derive(Default)]
    struct CycleB;
    impl Plugin for CycleB {
        const ID: &'static str = "test.cycle-b";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new(CycleA::ID, 1)];
    }

    #[derive(Default)]
    struct EmptyId;
    impl Plugin for EmptyId {
        const ID: &'static str = "";
    }

    #[derive(Default)]
    struct ZeroVersion;
    impl Plugin for ZeroVersion {
        const ID: &'static str = "test.zero";
        const VERSION: u32 = 0;
    }

    #[test]
    fn invalid_compositions_fail_before_starting_any_instance() {
        fn with<P: Plugin>() -> Registry {
            let mut registry = Registry::new();
            registry.plugin::<Base>().plugin::<P>();
            registry
        }
        let mut cycle = with::<CycleA>();
        cycle.plugin::<CycleB>();
        let cases = [
            (with::<Duplicate>(), "test.base is registered twice"),
            (
                with::<Missing>(),
                "test.missing-consumer requires test.absent",
            ),
            (with::<Incompatible>(), "test.base contract 2, found 1"),
            (cycle, "test.cycle-a -> test.cycle-b -> test.cycle-a"),
            (with::<EmptyId>(), "needs an ID"),
            (with::<ZeroVersion>(), "nonzero contract version"),
        ];
        for (registry, diagnostic) in cases {
            let (system, trace) = system();
            let error = system.start_plugins(&[registry]).unwrap_err();
            assert!(error.contains(diagnostic), "{error}");
            assert!(
                trace.0.borrow().is_empty(),
                "validation must precede all starts"
            );
            assert!(system.plugin::<Base>().is_err());
            system.locals.close_all();
        }
    }

    #[derive(Default)]
    struct Fault<const PANIC: bool> {
        trace: RefCell<Option<Rc<Trace>>>,
    }

    impl<const PANIC: bool> Plugin for Fault<PANIC> {
        const ID: &'static str = "test.fault";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new(Base::ID, 1)];

        fn start(&self, system: &System) -> Result<(), String> {
            let trace = system.locals.get::<Trace>();
            trace.0.borrow_mut().push("fault.start");
            self.trace.replace(Some(trace));
            if PANIC {
                panic!("initialization panicked");
            }
            Err("initialization failed".into())
        }

        fn stop(&self) {
            self.trace
                .borrow()
                .as_ref()
                .unwrap()
                .0
                .borrow_mut()
                .push("fault.stop");
        }
    }

    #[test]
    fn failed_or_panicking_start_releases_partial_work_and_rolls_back_dependencies() {
        fn check<const PANIC: bool>() {
            let (system, trace) = system();
            let mut registry = Registry::new();
            registry.plugin::<Fault<PANIC>>().plugin::<Base>();
            let error = system
                .start_plugins(std::slice::from_ref(&registry))
                .unwrap_err();
            assert!(
                error.contains("plugin test.fault: initialization"),
                "{error}"
            );
            assert_eq!(
                &*trace.0.borrow(),
                &["base.start", "fault.start", "fault.stop", "base.stop"]
            );
            assert!(system.plugin::<Base>().is_err());
            assert!(system.plugin::<Fault<PANIC>>().is_err());
            assert!(
                system
                    .start_plugins(&[registry])
                    .unwrap_err()
                    .contains("stopped")
            );
            system.locals.close_all();
            assert_eq!(
                trace.0.borrow().len(),
                4,
                "rollback is not repeated at VM disposal"
            );
        }
        check::<false>();
        check::<true>();
    }

    struct FactoryPanic;
    impl Default for FactoryPanic {
        fn default() -> Self {
            panic!("factory panicked");
        }
    }
    impl Plugin for FactoryPanic {
        const ID: &'static str = "test.factory";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new(Base::ID, 1)];
    }

    #[test]
    fn factory_panic_rolls_back_ready_dependencies() {
        let (system, trace) = system();
        let mut registry = Registry::new();
        registry.plugin::<FactoryPanic>().plugin::<Base>();
        let error = system.start_plugins(&[registry]).unwrap_err();
        assert!(
            error.contains("plugin test.factory: factory panicked"),
            "{error}"
        );
        assert_eq!(&*trace.0.borrow(), &["base.start", "base.stop"]);
        assert!(system.plugin::<Base>().is_err());
        system.locals.close_all();
        assert_eq!(trace.0.borrow().len(), 2);
    }

    #[derive(Default)]
    struct StopPanic {
        base: RefCell<Option<Rc<Base>>>,
    }
    impl Plugin for StopPanic {
        const ID: &'static str = "test.stop-panic";
        const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new(Base::ID, 1)];

        fn start(&self, system: &System) -> Result<(), String> {
            self.base.replace(Some(system.plugin::<Base>().unwrap()));
            Ok(())
        }

        fn stop(&self) {
            let base = self.base.borrow_mut().take().unwrap();
            base.trace
                .borrow()
                .as_ref()
                .unwrap()
                .0
                .borrow_mut()
                .push("panic.stop");
            panic!("cleanup panicked");
        }
    }

    #[test]
    fn cleanup_continues_after_a_plugin_panics() {
        let (system, trace) = system();
        let mut registry = Registry::new();
        registry.plugin::<StopPanic>().plugin::<Base>();
        system.start_plugins(&[registry]).unwrap();
        system.locals.close_all();
        system.locals.close_all();
        assert_eq!(
            &*trace.0.borrow(),
            &["base.start", "panic.stop", "base.stop"]
        );
    }

    #[derive(Default)]
    struct StopsDuringStart;
    impl Plugin for StopsDuringStart {
        const ID: &'static str = "test.stop-during-start";

        fn start(&self, system: &System) -> Result<(), String> {
            system.locals.close_all();
            Ok(())
        }
    }

    #[test]
    fn closing_the_vm_during_initialization_never_publishes_the_instance() {
        let (system, _) = system();
        let mut registry = Registry::new();
        registry.plugin::<StopsDuringStart>();
        let error = system.start_plugins(&[registry]).unwrap_err();
        assert!(error.contains("stopped during initialization"), "{error}");
        assert!(system.plugin::<StopsDuringStart>().is_err());
        assert!(system.start_plugins(&[]).unwrap_err().contains("stopped"));
    }

    #[derive(Default)]
    struct CleanupLookup {
        system: RefCell<Option<System>>,
        trace: RefCell<Option<Rc<Trace>>>,
    }
    impl Plugin for CleanupLookup {
        const ID: &'static str = "test.cleanup-lookup";

        fn start(&self, system: &System) -> Result<(), String> {
            self.system.replace(Some(system.clone()));
            self.trace.replace(Some(system.locals.get::<Trace>()));
            Ok(())
        }

        fn stop(&self) {
            let system = self.system.borrow_mut().take().unwrap();
            assert!(system.plugin::<Self>().is_err());
            assert!(system.start_plugins(&[]).unwrap_err().contains("stopped"));
            self.trace
                .borrow()
                .as_ref()
                .unwrap()
                .0
                .borrow_mut()
                .push("lookup.stop");
        }
    }

    #[test]
    fn cleanup_cannot_recreate_a_plugin_manager() {
        let (system, trace) = system();
        let mut registry = Registry::new();
        registry.plugin::<CleanupLookup>();
        system.start_plugins(&[registry]).unwrap();
        system.locals.close_all();
        assert_eq!(&*trace.0.borrow(), &["lookup.stop"]);
        assert!(system.plugin::<CleanupLookup>().is_err());
    }

    #[test]
    fn submodule_plugins_join_the_same_dependency_graph() {
        let (system, trace) = system();
        let mut parent = Registry::ui_set("test/plugins");
        let mut child = Registry::ui_set("test/plugins/consumer");
        child.plugin::<Consumer>();
        parent.plugin::<Base>().submodule(child);
        system.start_plugins(&[parent]).unwrap();
        assert_eq!(&*trace.0.borrow(), &["base.start", "consumer.start"]);
        system.locals.close_all();
    }
}
