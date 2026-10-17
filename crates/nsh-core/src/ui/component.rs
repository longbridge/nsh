//! Components: the [`Component`] trait, its [`Props`] and [`Event`]s
//! (derived), the [`Registry`] a surface renders from, and the JS glue and
//! TypeScript declarations generated from them ([`declarations`]).

use std::{
    any::{Any, TypeId},
    fmt,
    sync::Arc,
};

use rustc_hash::FxHashMap;

use super::{
    Children, IntoElement, PropError, Slot, TsType, TsTypes,
    element::{ComponentElement, ElNode},
    module::{Function, FunctionSpec, Module, ModuleSpec},
};
use crate::JsonValue;

/// A component written in Rust: a function that runs once ([`Self::view`])
/// and makes its view, binding what changes to signals. Its props come from
/// JS (or from the component whose view has it), its events go to JS.
/// `#[component]` on a function makes the type and this impl.
///
/// It is registered under its type's name ([`std::any::type_name`]:
/// `nsh_ui_mobile::button::Button`) and exported to JS as the type's own
/// name (`Button`), so neither is written by hand
/// ([`Registry::register`]).
///
/// ```ignore
/// #[component]
/// pub fn Button(props: ButtonProps, cx: Scope<ButtonEvent>) -> impl IntoElement {
///     view()
///         .px_4()
///         .py_3()
///         .bg(move || theme().primary)
///         .on_press(move |_: &()| cx.emit(ButtonEvent::Press))
///         .child(text(props.title))
/// }
/// ```
pub trait Component: 'static {
    /// Its props (`#[derive(Props)]`).
    type Props: Props;

    /// Its events (`#[derive(Event)]`; [`NoEvent`] for none).
    type Event: Event;

    /// The methods of its ref in JS: those of the [`Api`] its view
    /// registers (`Methods::of::<StepperApi>()`; none by default).
    const METHODS: Methods = Methods::NONE;

    /// What it shows: its elements, with bindings where values change (TS:
    /// a component's body). Runs once, under an owner of its own: what it
    /// makes (signals, effects, timers) goes with the component.
    fn view(props: Self::Props, cx: super::Scope<Self::Event>) -> impl IntoElement;

    /// This component inside another one's view.
    fn element(props: Self::Props) -> ComponentElement<Self>
    where
        Self: Sized,
    {
        ComponentElement::new(props)
    }
}

/// A component's code panicked: what an error boundary
/// ([`super::catch_error`]) shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentError {
    /// The component that failed (its registered name: its type's).
    pub component: &'static str,
    /// What its code panicked with.
    pub message: String,
}

impl fmt::Display for ComponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed: {}", self.component, self.message)
    }
}

/// Runs component code, catching a panic as its message.
pub(crate) fn caught<R>(f: impl FnOnce() -> R) -> Result<R, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(panic_message)
}

/// The message of a panic's payload.
pub(crate) fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(s) => (*s).to_owned(),
            Err(_) => "a panic".to_owned(),
        },
    }
}

/// A method JS calls on a component's ref: one of its API's
/// ([`Api`]; `#[methods]` makes them).
pub struct Method<A: 'static> {
    /// Its JS name (`setValue`).
    pub name: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Its parameters: JS names and TypeScript types.
    pub params: &'static [Param],
    /// The TypeScript type of its result (`None`: it returns nothing, and
    /// JS does not wait for it).
    pub returns: Option<TsType>,
    /// Calls it with JS's arguments.
    pub call: fn(&A, Vec<JsonValue>) -> Result<JsonValue, PropError>,
}

/// What a component's ref does in JS: a handle its view makes, holding the
/// signals its methods read and write, registered with
/// [`super::Scope::methods`]. `#[methods]` on its `impl` block implements
/// it.
pub trait Api: Sized + 'static {
    /// Its methods.
    const METHODS: &'static [Method<Self>];
}

/// The methods a component's ref has, for its spec and declarations:
/// [`Methods::of`] its [`Api`] (`#[component(methods = StepperApi)]`).
#[derive(Clone, Copy)]
pub struct Methods {
    specs: fn(&mut TsTypes) -> Vec<MethodSpec>,
}

impl Methods {
    /// None.
    pub const NONE: Self = Self {
        specs: |_| Vec::new(),
    };

    /// Those of `A`.
    pub const fn of<A: Api>() -> Self {
        Self {
            specs: specs_of::<A>,
        }
    }
}

fn specs_of<A: Api>(types: &mut TsTypes) -> Vec<MethodSpec> {
    A::METHODS
        .iter()
        .map(|m| MethodSpec {
            name: m.name,
            doc: m.doc,
            params: m.params.iter().map(|(n, ts)| (*n, ts(types))).collect(),
            returns: m.returns.map(|ts| ts(types)),
        })
        .collect()
}

/// A method's parameter: its JS name, and its TypeScript type.
pub type Param = (&'static str, TsType);

/// One method, for the glue and its declarations.
#[derive(Debug, Clone)]
pub struct MethodSpec {
    /// Its JS name.
    pub name: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Its parameters: JS names and TypeScript types.
    pub params: Vec<(&'static str, String)>,
    /// The TypeScript type of its result; `None`: nothing to wait for.
    pub returns: Option<String>,
}

/// What a prop is to the glue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    /// A value, sent as `$name`.
    Value,
    /// A named slot: JSX in a holder node.
    Slot,
    /// The JSX children.
    Children,
    /// Virtualized rows: in JS `each`, a row renderer and the data props of
    /// `List` (the glue makes the list, the component places it).
    Items,
}

/// One prop, for the glue and its declarations.
#[derive(Debug, Clone)]
pub struct PropSpec {
    /// Its JS name (`accessibilityLabel`).
    pub name: &'static str,
    /// Its TypeScript type.
    pub ts: String,
    /// It may be left out.
    pub optional: bool,
    /// Documentation.
    pub doc: &'static str,
    /// What it is.
    pub kind: PropKind,
}

/// One event, for the glue and its declarations.
#[derive(Debug, Clone)]
pub struct EventSpec {
    /// Its name (`press`: the handler is `onPress`).
    pub name: &'static str,
    /// The TypeScript type of its data (`void` for none).
    pub data: String,
    /// Documentation.
    pub doc: &'static str,
}

/// A component's props. Derive it: `#[derive(Props)]`.
pub trait Props: Clone + PartialEq + 'static {
    /// The JS name of the [`super::Items`] prop, if it has one.
    #[doc(hidden)]
    const ITEMS: Option<&'static str> = None;

    /// Its children are made only while the view places them
    /// (`#[prop(lazy)]`).
    #[doc(hidden)]
    const LAZY_CHILDREN: bool = false;

    /// The component's documentation (the struct's doc comment).
    fn doc() -> &'static str;

    /// The props (their named types declared in `types`).
    fn spec(types: &mut TsTypes) -> Vec<PropSpec>;

    /// Every prop at its default.
    fn initial() -> Self;

    /// Prop `name` (its JS name) from JS; `None` restores its default.
    /// Returns whether it changed.
    fn set(&mut self, name: &str, value: Option<JsonValue>) -> Result<bool, PropError>;

    /// Slot `name` filled (`Some`) or not; returns whether it changed.
    fn set_slot(&mut self, name: &str, slot: Option<Slot>) -> bool;

    /// Whether the app passed children; returns whether it changed.
    fn set_children(&mut self, children: Children) -> bool;

    /// The props an instance keeps: each [`super::Prop`] field held in a
    /// signal of the current owner, so new values reach only what read it.
    #[doc(hidden)]
    fn held(&self) -> Self {
        self.clone()
    }

    /// An instance's props take `new`'s (a parent's region made the
    /// element again): held props update their readers; returns whether
    /// another field changed.
    #[doc(hidden)]
    fn update_from(&mut self, new: &Self) -> bool {
        if self == new {
            return false;
        }
        *self = new.clone();
        true
    }
}

/// The props of a component that has none (an app's root, say).
impl Props for () {
    fn doc() -> &'static str {
        ""
    }

    fn spec(_: &mut TsTypes) -> Vec<PropSpec> {
        Vec::new()
    }

    fn initial() -> Self {}

    fn set(&mut self, name: &str, _: Option<JsonValue>) -> Result<bool, PropError> {
        Err(PropError(format!("unknown prop `{name}`")))
    }

    fn set_slot(&mut self, _: &str, _: Option<Slot>) -> bool {
        false
    }

    fn set_children(&mut self, _: Children) -> bool {
        false
    }
}

/// A component's events. Derive it: `#[derive(Event)]`.
pub trait Event: 'static {
    /// The events (their named types declared in `types`).
    fn spec(types: &mut TsTypes) -> Vec<EventSpec>;

    /// This event's name.
    fn name(&self) -> &'static str;

    /// Its data for JS.
    fn data(&self) -> JsonValue;
}

/// The events of a component that has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoEvent {}

impl Event for NoEvent {
    fn spec(_types: &mut TsTypes) -> Vec<EventSpec> {
        Vec::new()
    }

    fn name(&self) -> &'static str {
        match *self {}
    }

    fn data(&self) -> JsonValue {
        match *self {}
    }
}

/// What a component is to JS: its glue and declarations are generated
/// from it ([`declarations`]).
#[derive(Debug, Clone)]
pub struct ComponentSpec {
    /// Its registered name: its type's ([`std::any::type_name`]:
    /// `nsh_ui_mobile::button::Button`), unique within one binary.
    pub name: &'static str,
    /// Its JS export: the type's own name (`Button`).
    pub export: String,
    /// Documentation.
    pub doc: &'static str,
    /// Its props, slots and children.
    pub props: Vec<PropSpec>,
    /// Its events.
    pub events: Vec<EventSpec>,
    /// The methods of its ref.
    pub methods: Vec<MethodSpec>,
    /// The named TypeScript types they use.
    pub types: TsTypes,
    /// JS makes the children only while the view places them
    /// (`#[prop(lazy)]` on its `Children`): the core tells it with a
    /// `$children` event on the host.
    pub lazy_children: bool,
}

impl ComponentSpec {
    pub(crate) fn of<C: Component>() -> Self {
        let mut types = TsTypes::default();
        let props = C::Props::spec(&mut types);
        let events = C::Event::spec(&mut types);
        let methods = (C::METHODS.specs)(&mut types);
        let name = name_of::<C>();
        Self {
            name,
            export: export_name(name).to_owned(),
            doc: C::Props::doc(),
            props,
            events,
            methods,
            types,
            lazy_children: C::Props::LAZY_CHILDREN,
        }
    }

    /// Its named slots.
    pub fn slots(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.props
            .iter()
            .filter(|p| p.kind == PropKind::Slot)
            .map(|p| p.name)
    }

    /// Whether it takes JSX children.
    pub fn takes_children(&self) -> bool {
        self.props.iter().any(|p| p.kind == PropKind::Children)
    }

    /// Its [`PropKind::Items`] prop (one at most).
    pub fn items(&self) -> Option<&'static str> {
        self.props
            .iter()
            .find(|p| p.kind == PropKind::Items)
            .map(|p| p.name)
    }

    /// The nodes the glue makes for it, by name: slots and the list.
    pub(crate) fn holder(&self, name: &str) -> Option<&'static str> {
        self.props
            .iter()
            .find(|p| matches!(p.kind, PropKind::Slot | PropKind::Items) && p.name == name)
            .map(|p| p.name)
    }
}

/// The registered name of component `C`: its type's. It is only compared
/// within one binary, so the format of [`std::any::type_name`] may change
/// between compilers; the exports, which JS sees, are pinned by tests.
pub(crate) fn name_of<C: Component>() -> &'static str {
    std::any::type_name::<C>()
}

/// A type's own name, its JS export: `nsh_ui_mobile::tab_bar::TabBar` ->
/// `TabBar` (a generic one's without its arguments: `app::Picker<app::Fruit>`
/// -> `Picker`).
fn export_name(type_name: &str) -> &str {
    let path = type_name.split('<').next().unwrap_or(type_name);
    path.rsplit("::").next().unwrap_or(path)
}

/// An instance: a component's props and view, their types erased.
pub(crate) trait AnyInstance {
    fn set_prop(&mut self, name: &str, value: Option<JsonValue>) -> Result<bool, PropError>;
    /// Props from the component whose view has this one.
    fn set_props(&mut self, props: &dyn Any);
    fn set_slot(&mut self, name: &'static str, filled: bool);
    fn set_children(&mut self, present: bool);
    /// The component goes away: what its view made goes with it.
    fn cleanup(&mut self);
    /// Its view, made once.
    fn view(&mut self) -> ElNode;
    /// Its registered name.
    fn name(&self) -> &'static str;
    /// Method `name` of its ref; `None` when it has none of that name.
    fn call(&mut self, name: &str, args: Vec<JsonValue>) -> Option<Result<JsonValue, PropError>>;
    /// Its view's owner, once its view is made.
    fn owner(&self) -> Option<super::Owner>;
    /// Where a failure of its code goes first: its owner, or where it is
    /// made before it has one.
    fn failure_owner(&self) -> super::Owner;
    /// Made by JS (where no owner is): its view will be made under
    /// `parent`, the owner of the nearest component above it in the tree,
    /// so it sees that one's context.
    fn adopt(&mut self, parent: super::Owner);
    /// It is the component of `host`, made where `parent` owns.
    fn place(&mut self, host: crate::NodeId, parent: super::Owner);
}

/// The root of a Rust app: makes the app's top component (on the engine
/// thread, where elements live). The engine mounts it as the content of the
/// surface (`nsh_engine::App::Rust`).
#[derive(Clone)]
pub struct Root {
    make: Arc<dyn Fn() -> super::AnyElement + Send + Sync>,
    theme: Option<Arc<super::Theme>>,
    fonts: Vec<Arc<[u8]>>,
}

impl Root {
    /// Component `C` with the props `props` makes.
    pub fn new<C: Component>(props: impl Fn() -> C::Props + Send + Sync + 'static) -> Self {
        Self {
            make: Arc::new(move || super::IntoElement::into_element(C::element(props()))),
            theme: None,
            fonts: Vec::new(),
        }
    }

    /// Renders with `theme` (by default the default one).
    pub fn theme(mut self, theme: super::Theme) -> Self {
        self.theme = Some(Arc::new(theme));
        self
    }

    /// Registers a font file (TrueType / OpenType) before the first render;
    /// styles name it by its family name.
    pub fn font(mut self, data: impl Into<Arc<[u8]>>) -> Self {
        self.fonts.push(data.into());
        self
    }

    /// The theme it renders with.
    pub fn theme_ref(&self) -> Option<&super::Theme> {
        self.theme.as_deref()
    }

    /// The font files to register.
    pub fn fonts(&self) -> &[Arc<[u8]>] {
        &self.fonts
    }

    /// Component `C` with default props.
    pub fn of<C: Component>() -> Self
    where
        C::Props: Default,
    {
        Self::new::<C>(C::Props::default)
    }

    /// The element of the root component.
    pub(crate) fn element(&self) -> super::AnyElement {
        (self.make)()
    }
}

impl fmt::Debug for Root {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Root")
    }
}

/// A new instance of `C`, its props not set yet.
pub(crate) fn make_instance<C: Component>() -> Box<dyn AnyInstance> {
    Box::new(super::once::OnceInstance::<C>::new())
}

#[derive(Clone)]
struct ComponentType {
    spec: Arc<ComponentSpec>,
    make: fn() -> Box<dyn AnyInstance>,
    /// Tells two types of the same name apart (two versions of a crate).
    id: TypeId,
}

#[derive(Clone)]
struct ModuleType {
    spec: Arc<ModuleSpec>,
    functions: &'static [Function],
    /// The type that implements it.
    type_name: &'static str,
}

/// What a UI set exports to JS: its components and modules, and what it
/// passes on from the runtime. Cheap to clone.
#[derive(Clone, Default)]
pub struct Registry {
    types: Arc<FxHashMap<&'static str, ComponentType>>,
    modules: Arc<FxHashMap<&'static str, ModuleType>>,
    /// Values and types of `nsh/_core` exported as they are.
    reexports: Arc<(Vec<&'static str>, Vec<&'static str>)>,
    /// Values `nsh-js` makes for the set (declared by hand, not generated
    /// from a spec).
    native_values: Arc<Vec<&'static str>>,
    /// The runtime module the functions belong to (`nsh/android`): JS
    /// calls them as `nsh/android:Toast.show`, apart from any UI set's.
    namespace: Option<&'static str>,
    /// The JS module a UI set is imported as (`nsh/ui/mobile`).
    ui_set: Option<&'static str>,
    /// The files its crates ship ([`Registry::assets`]).
    assets: Arc<Vec<&'static [super::AssetFile]>>,
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.types.keys()).finish()
    }
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers component `C` under its type's name, exported to JS as
    /// the type's own name ([`ComponentSpec::name`], [`ComponentSpec::export`]).
    ///
    /// # Panics
    ///
    /// When `C` is registered already, another type of the same name is
    /// (two versions of one crate), or another component of the set has the
    /// same export: a mistake in the code, caught by the app's first run.
    #[track_caller]
    pub fn register<C: Component>(&mut self) -> &mut Self {
        let spec = ComponentSpec::of::<C>();
        let types = Arc::make_mut(&mut self.types);
        if let Some(old) = types.get(spec.name) {
            let set = self.ui_set.or(self.namespace);
            panic!("{}", same_name(spec.name, old.id, TypeId::of::<C>(), set));
        }
        if let Some(other) = types.values().find(|t| t.spec.export == spec.export) {
            panic!(
                "components `{}` and `{}` are both exported as `{}`{}: rename one",
                other.spec.name,
                spec.name,
                spec.export,
                of_set(self.ui_set.or(self.namespace)),
            );
        }
        types.insert(
            spec.name,
            ComponentType {
                spec: Arc::new(spec),
                make: make_instance::<C>,
                id: TypeId::of::<C>(),
            },
        );
        self
    }

    /// Registers module `M`: its functions on a JS object named
    /// [`Module::NAME`] (merged into the component exported by that name, if
    /// the set has one).
    ///
    /// # Panics
    ///
    /// When the set has a module of that name already.
    #[track_caller]
    pub fn module<M: Module>(&mut self) -> &mut Self {
        let type_name = std::any::type_name::<M>();
        let modules = Arc::make_mut(&mut self.modules);
        if let Some(old) = modules.get(M::NAME) {
            panic!(
                "{}",
                same_module(
                    M::NAME,
                    old.type_name,
                    type_name,
                    self.ui_set.or(self.namespace)
                )
            );
        }
        modules.insert(
            M::NAME,
            ModuleType {
                spec: Arc::new(ModuleSpec::of::<M>()),
                functions: M::FUNCTIONS,
                type_name,
            },
        );
        self
    }

    /// Exports `values` and `types` of the runtime as they are (accessors
    /// such as `safeArea`, hooks, types such as `Insets`).
    pub fn reexport(&mut self, values: &[&'static str], types: &[&'static str]) -> &mut Self {
        let r = Arc::make_mut(&mut self.reexports);
        r.0.extend_from_slice(values);
        r.1.extend_from_slice(types);
        self
    }

    /// The components and modules of all of `sets`, for finding them by
    /// name (the core's view of the sets a host serves). Not a module of its
    /// own. Components of different sets may share an export (JS imports
    /// them from different modules), not a name.
    ///
    /// # Panics
    ///
    /// When two sets register the same component, two types of the same
    /// name, or modules of the same name (JS calls their functions by it).
    #[track_caller]
    pub fn merged(sets: &[Registry]) -> Self {
        let mut out = Self::default();
        for set in sets {
            let types = Arc::make_mut(&mut out.types);
            for (name, ty) in set.types.iter() {
                if let Some(old) = types.insert(name, ty.clone()) {
                    panic!("{}, by two UI sets", same_name(name, old.id, ty.id, None));
                }
            }
            let modules = Arc::make_mut(&mut out.modules);
            for (name, m) in set.modules.iter() {
                if let Some(old) = modules.insert(name, m.clone()) {
                    panic!(
                        "{}, by two UI sets",
                        same_module(name, old.type_name, m.type_name, None)
                    );
                }
            }
        }
        out
    }

    /// The registry of a UI set apps import as `specifier`
    /// (`nsh/ui/mobile`). An app's crate makes its own with
    /// `#[native_module("…")]`.
    pub fn ui_set(specifier: &'static str) -> Self {
        Self {
            ui_set: Some(specifier),
            ..Self::default()
        }
    }

    /// The JS module the registry is imported as: a UI set's, or the
    /// runtime module's (`None`: a Rust app's components).
    pub fn specifier(&self) -> Option<&'static str> {
        self.ui_set.or(self.namespace)
    }

    /// The runtime values and types exported as they are
    /// ([`Registry::reexport`]).
    pub fn reexports(&self) -> (&[&'static str], &[&'static str]) {
        (&self.reexports.0, &self.reexports.1)
    }

    /// Exports `values` `nsh-js` makes for the set (`Stack`, `Screen`),
    /// declared by hand rather than generated from a spec.
    pub fn native_values(&mut self, values: &[&'static str]) -> &mut Self {
        Arc::make_mut(&mut self.native_values).extend_from_slice(values);
        self
    }

    /// Ships `files` with the apps that use the set (`assets!()`: a crate's
    /// `assets/` directory): `nsh run` serves them, `nsh package` copies
    /// them into the app, and the engine registers the fonts among them
    /// before the first render. `asset!` refers to one.
    pub fn assets(&mut self, files: &'static [super::AssetFile]) -> &mut Self {
        Arc::make_mut(&mut self.assets).push(files);
        self
    }

    /// The files [`Registry::assets`] ships.
    pub fn asset_files(&self) -> impl Iterator<Item = &'static super::AssetFile> + '_ {
        self.assets.iter().flat_map(|files| files.iter())
    }

    /// The values of [`Registry::native_values`].
    pub fn native_value_names(&self) -> &[&'static str] {
        &self.native_values
    }

    /// A registry of the runtime's module `namespace` (`nsh/android`).
    pub(crate) fn runtime(namespace: &'static str) -> Self {
        Self {
            namespace: Some(namespace),
            ..Self::default()
        }
    }

    /// The runtime module the registry's functions belong to (`None`: a UI
    /// set's).
    pub fn namespace(&self) -> Option<&'static str> {
        self.namespace
    }

    /// Function `name` of a module (`Alert.alert`; with a namespace,
    /// `nsh/android:Toast.show`).
    pub(crate) fn function(&self, name: &str) -> Option<&'static Function> {
        let name = match self.namespace {
            Some(namespace) => name.strip_prefix(namespace)?.strip_prefix(':')?,
            None => name,
        };
        let (module, function) = name.split_once('.')?;
        self.modules
            .get(module)?
            .functions
            .iter()
            .find(|f| f.name == function)
    }

    /// The registered modules, by name.
    pub fn module_specs(&self) -> Vec<Arc<ModuleSpec>> {
        let mut specs: Vec<_> = self.modules.values().map(|m| m.spec.clone()).collect();
        specs.sort_by_key(|s| s.name);
        specs
    }

    /// The registered components, by export.
    pub fn specs(&self) -> Vec<Arc<ComponentSpec>> {
        let mut specs: Vec<_> = self.types.values().map(|t| t.spec.clone()).collect();
        specs.sort_by(|a, b| a.export.cmp(&b.export).then(a.name.cmp(b.name)));
        specs
    }

    /// A new instance of component `name`.
    pub(crate) fn make(&self, name: &str) -> Option<(Arc<ComponentSpec>, Box<dyn AnyInstance>)> {
        self.types.get(name).map(|t| (t.spec.clone(), (t.make)()))
    }
}

/// The message of two registrations under component name `name`.
fn same_name(name: &str, old: TypeId, new: TypeId, set: Option<&str>) -> String {
    if old == new {
        format!("component `{name}` is registered twice{}", of_set(set))
    } else {
        format!(
            "two different types named `{name}` are registered{} (two versions of one crate?)",
            of_set(set)
        )
    }
}

/// The message of two modules named `name`.
fn same_module(name: &str, old: &str, new: &str, set: Option<&str>) -> String {
    if old == new {
        format!("module `{old}` is registered twice{}", of_set(set))
    } else {
        format!(
            "modules `{old}` and `{new}` are both named `{name}`{}: rename one",
            of_set(set)
        )
    }
}

/// ` in `nsh/ui/mobile`` for a registry with a specifier.
fn of_set(set: Option<&str>) -> String {
    set.map(|s| format!(" in `{s}`")).unwrap_or_default()
}

/// The declarations of what `registry`'s specs export: one component per
/// spec (by its [`ComponentSpec::export`] name), one object per module
/// (its type `<Name>Module`; merged into a component of the same name; a
/// flat module's functions one by one), and the named types they use.
/// Exports named in `omit` are left out (their types stay): `nsh-js`
/// declares them, building on the module (`BackHandlerModule & ...`), as
/// it declares the re-exports and native values it makes.
pub fn declarations(registry: &Registry, omit: &[&str]) -> String {
    let specs = registry.specs();
    let modules = registry.module_specs();
    use std::fmt::Write as _;
    let mut out = String::new();
    let doc = |out: &mut String, indent: &str, doc: &str| {
        if !doc.is_empty() {
            let _ = writeln!(out, "{indent}/** {doc} */");
        }
    };
    // The named types, once each.
    let mut types = TsTypes::default();
    for spec in &specs {
        types.merge(&spec.types);
    }
    for m in &modules {
        types.merge(&m.types);
    }
    for declaration in types.declarations() {
        let _ = write!(out, "{declaration}\n\n");
    }
    let module_of = |name: &str| modules.iter().find(|m| m.name == name);
    let signature = |f: &FunctionSpec| {
        let params = f
            .params
            .iter()
            .map(|(n, ts, optional)| format!("{n}{}: {ts}", if *optional { "?" } else { "" }))
            .collect::<Vec<_>>()
            .join(", ");
        let returns = match &f.returns {
            Some(ts) => format!("Promise<{ts}>"),
            None => "void".to_owned(),
        };
        (params, returns)
    };
    // A module's object type.
    let module_object = |out: &mut String, m: &ModuleSpec| {
        out.push_str("{\n");
        for f in &m.functions {
            doc(out, "  ", f.doc);
            let (params, returns) = signature(f);
            let _ = writeln!(out, "  {}({params}): {returns};", f.name);
        }
        out.push('}');
    };
    for spec in &specs {
        // A component with rows is generic over their type, like `List`.
        let (generic, extends) = match spec.items() {
            Some(_) => ("<T>", ", ListItemsProps<T>"),
            None => ("", ""),
        };
        // A component with methods: its ref is an object of them.
        let host = if spec.methods.is_empty() {
            "ComponentHostProps"
        } else {
            let _ = write!(
                out,
                "/** The ref of `{0}`: its methods. */\nexport interface {0}Ref {{\n  /** The component's host node. */\n  readonly node: YNode;\n",
                spec.export
            );
            for m in &spec.methods {
                doc(&mut out, "  ", m.doc);
                let params = m
                    .params
                    .iter()
                    .map(|(n, ts)| format!("{n}: {ts}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let returns = match &m.returns {
                    Some(ts) => format!("Promise<{ts}>"),
                    None => "void".to_owned(),
                };
                let _ = writeln!(out, "  {}({params}): {returns};", m.name);
            }
            out.push_str("}\n\n");
            "Omit<ComponentHostProps, \"ref\">"
        };
        let _ = writeln!(
            out,
            "export interface {}Props{generic} extends {host}{extends} {{",
            spec.export
        );
        if !spec.methods.is_empty() {
            let _ = writeln!(
                out,
                "  ref?: {0}Ref | ((ref: {0}Ref) => void);",
                spec.export
            );
        }
        for p in spec.props.iter().filter(|p| p.kind != PropKind::Items) {
            doc(&mut out, "  ", p.doc);
            let optional = if p.optional { "?" } else { "" };
            let _ = writeln!(out, "  {}{optional}: {};", p.name, p.ts);
        }
        for e in &spec.events {
            doc(&mut out, "  ", e.doc);
            let handler = handler_name(e.name);
            let arg = if e.data == "void" {
                String::new()
            } else {
                format!("data: {}", e.data)
            };
            let _ = writeln!(out, "  {handler}?: ({arg}) => void;");
        }
        out.push_str("}\n\n");
        if omit.contains(&spec.export.as_str()) {
            continue;
        }
        doc(&mut out, "", spec.doc);
        // A module of the same name: its functions on the component.
        let mut with = String::new();
        if let Some(m) = module_of(&spec.export) {
            with.push_str(" & ");
            module_object(&mut with, m);
        }
        let _ = write!(
            out,
            "export declare const {}: ({generic}(props: {}Props{generic}) => JSX.Element){with};\n\n",
            spec.export, spec.export,
        );
    }
    for m in &modules {
        if specs.iter().any(|s| s.export == m.name) {
            continue;
        }
        if m.flat {
            for f in m.functions.iter().filter(|f| !omit.contains(&f.name)) {
                doc(&mut out, "", f.doc);
                let (params, returns) = signature(f);
                let _ = write!(
                    out,
                    "export declare function {}({params}): {returns};\n\n",
                    f.name
                );
            }
            continue;
        }
        doc(&mut out, "", m.doc);
        let _ = write!(out, "export interface {}Module ", m.name);
        module_object(&mut out, m);
        out.push_str("\n\n");
        if !omit.contains(&m.name) {
            doc(&mut out, "", m.doc);
            let _ = write!(out, "export declare const {0}: {0}Module;\n\n", m.name);
        }
    }
    out
}

/// `press` -> `onPress`.
fn handler_name(event: &str) -> String {
    let mut chars = event.chars();
    match chars.next() {
        Some(first) => format!("on{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => "on".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::prelude::*;

    #[derive(Default)]
    struct Stepper;

    /// Picks a number.
    #[derive(Clone, PartialEq, Props)]
    struct StepperProps {
        /// The value.
        value: f64,
        #[prop(default)]
        size: StepperSize,
        max_value: Option<f64>,
        /// Above the control.
        label: Option<Slot>,
        children: Children,
    }

    #[derive(Clone, Copy, PartialEq, Default, PropEnum)]
    enum StepperSize {
        #[default]
        Regular,
        ExtraLarge,
    }

    #[derive(Event)]
    enum StepperEvent {
        /// The value changed.
        Change {
            new_value: f64,
        },
        Limit(u32),
        Reset,
    }

    struct StepperApi;

    #[methods]
    impl StepperApi {
        /// Jumps to `value`.
        fn jump_to(&self, value: f64) {
            let _ = value;
        }

        fn value(&self) -> f64 {
            0.0
        }
    }

    impl Component for Stepper {
        type Props = StepperProps;
        type Event = StepperEvent;
        const METHODS: Methods = Methods::of::<StepperApi>();

        fn view(_: StepperProps, _: super::super::Scope<StepperEvent>) -> impl IntoElement {
            view()
        }
    }

    #[test]
    fn the_declarations_come_from_the_types() {
        let mut registry = Registry::new();
        registry.register::<Stepper>();
        let ts = declarations(&registry, &[]);
        for line in [
            "  /** The value. */\n  value: number;",
            "export type StepperSize = \"regular\" | \"extraLarge\";",
            "  size?: StepperSize;",
            "  maxValue?: number | null;",
            "  /** Above the control. */\n  label?: JSX.Element;",
            "  children?: JSX.Element;",
            "  /** The value changed. */\n  onChange?: (data: { newValue: number }) => void;",
            "  onLimit?: (data: number) => void;",
            "  onReset?: () => void;",
            "/** Picks a number. */",
            "export interface StepperRef {",
            "  /** Jumps to `value`. */\n  jumpTo(value: number): void;",
            "  value(): Promise<number>;",
            "export interface StepperProps extends Omit<ComponentHostProps, \"ref\"> {\n  ref?: StepperRef | ((ref: StepperRef) => void);",
            "export declare const Stepper: ((props: StepperProps) => JSX.Element);",
        ] {
            assert!(ts.contains(line), "missing {line:?} in\n{ts}");
        }
        let mut props = StepperProps::initial();
        assert!(
            props
                .set("size", Some(serde_json::json!("extraLarge")))
                .unwrap()
        );
        assert!(props.size == StepperSize::ExtraLarge);
        assert!(props.set("size", Some(serde_json::json!("huge"))).is_err());
        assert!(props.set("size", None).unwrap());
        assert!(props.size == StepperSize::Regular);
        assert_eq!(
            StepperEvent::Change { new_value: 2.5 }.data(),
            serde_json::json!({ "newValue": 2.5 })
        );
        assert_eq!(StepperEvent::Limit(3).data(), serde_json::json!(3));
        assert_eq!(StepperEvent::Reset.name(), "reset");
    }

    #[test]
    fn components_are_named_after_their_types() {
        let mut registry = Registry::ui_set("test/ui");
        registry.register::<Stepper>();
        let specs = registry.specs();
        assert_eq!(specs[0].name, "nsh_core::ui::component::tests::Stepper");
        assert_eq!(specs[0].export, "Stepper");
        assert_eq!(export_name("app::picker::Picker<app::Fruit>"), "Picker");
        assert_eq!(export_name("Bare"), "Bare");
    }

    mod other {
        use crate::ui::prelude::*;

        /// Another component exported as `Stepper`.
        #[derive(Default)]
        pub(super) struct Stepper;

        impl Component for Stepper {
            type Props = ();
            type Event = NoEvent;

            fn view(_: (), _: crate::ui::Scope<NoEvent>) -> impl IntoElement {
                view()
            }
        }
    }

    #[test]
    #[should_panic(
        expected = "components `nsh_core::ui::component::tests::Stepper` and `nsh_core::ui::component::tests::other::Stepper` are both exported as `Stepper` in `test/ui`"
    )]
    fn two_components_of_one_set_cannot_share_an_export() {
        Registry::ui_set("test/ui")
            .register::<Stepper>()
            .register::<other::Stepper>();
    }

    #[test]
    #[should_panic(
        expected = "component `nsh_core::ui::component::tests::Stepper` is registered twice"
    )]
    fn a_component_is_registered_once() {
        Registry::new().register::<Stepper>().register::<Stepper>();
    }

    #[test]
    fn sets_may_share_exports_but_not_components() {
        let mut a = Registry::ui_set("test/a");
        a.register::<Stepper>();
        let mut b = Registry::ui_set("test/b");
        b.register::<other::Stepper>();
        let merged = Registry::merged(&[a.clone(), b]);
        assert_eq!(merged.specs().len(), 2);
        let twice = std::panic::catch_unwind(|| Registry::merged(&[a.clone(), a]));
        let message = *twice.unwrap_err().downcast::<String>().unwrap();
        assert_eq!(
            message,
            "component `nsh_core::ui::component::tests::Stepper` is registered twice, by two UI sets"
        );
    }
}
