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
    call::{AnyValue, JsResult, MethodCall},
    class::{Class, ClassDef, ClassSpec, MemberKind, Thrown},
    element::{ComponentElement, ElNode},
    js::{JsHost, JsIn, JsOwned},
    module::{FunctionSpec, Module, ModuleSpec},
};

/// A component written in Rust: a function that runs once ([`Self::view`])
/// and makes its view, binding what changes to signals. Its props come from
/// JS (or from the component whose view has it), its events go to JS.
/// `#[component]` on a function makes the type and this impl.
///
/// It is registered under its type's full path ([`std::any::type_name`]:
/// its crate, modules and name) and exported to JS as the type's own name
/// (the last segment), so neither is written by hand
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

    /// Its documentation in TypeScript: the `#[component]` function's doc
    /// comment (none by default).
    const DOC: &'static str = "";

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
pub struct Method {
    /// Its JS name (`setValue`).
    pub name: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Its parameters: JS names and TypeScript types.
    pub params: &'static [Param],
    /// The TypeScript type of its result (`None`: it returns nothing, and
    /// JS gets `undefined`: the call runs in order with the props set
    /// before it, with nothing to wait for).
    pub returns: Option<TsType>,
    /// The classes its parameters and result name.
    pub refs: super::ClassRefs,
    /// Its call, the arguments converted in the JS call: what the core
    /// runs with the API, in order with the props set before it. A wrong
    /// argument's error starts with the method's name; the host puts its
    /// component's before it (`Stepper.setValue: ...`).
    pub call: fn(&dyn JsHost) -> Result<MethodCall, Thrown>,
}

/// What a component's ref does in JS: a handle its view makes, holding the
/// signals its methods read and write, registered with
/// [`super::Scope::methods`]. `#[methods]` on its `impl` block implements
/// it.
pub trait Api: Sized + 'static {
    /// Its methods.
    const METHODS: &'static [Method];
}

/// The methods a component's ref has, for its spec and declarations:
/// [`Methods::of`] its [`Api`] (`#[component(methods = StepperApi)]`).
#[derive(Clone, Copy)]
pub struct Methods {
    specs: fn(&mut TsTypes) -> Vec<MethodSpec>,
    refs: super::ClassRefs,
}

impl Methods {
    /// None.
    pub const NONE: Self = Self {
        specs: |_| Vec::new(),
        refs: |_| {},
    };

    /// Those of `A`.
    pub const fn of<A: Api>() -> Self {
        Self {
            specs: specs_of::<A>,
            refs: |out| {
                for m in A::METHODS {
                    (m.refs)(out);
                }
            },
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
            call: m.call,
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
    /// The TypeScript type of its result; `None`: nothing (`void`).
    pub returns: Option<String>,
    /// Its call (the arguments converted in the JS call).
    pub call: fn(&dyn JsHost) -> Result<MethodCall, Thrown>,
}

impl MethodSpec {
    /// Whether JS gets a `Promise` of the result: when the method returns
    /// a value (`Result<(), E>` too, rejected with an `Err`). One
    /// returning `()` gives `undefined` (`void`): the call runs in order
    /// with the UI and nothing answers it, and a wrong argument throws.
    pub fn promises(&self) -> bool {
        self.returns.is_some()
    }
}

/// What a prop is to the glue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    /// A value: decoded in the JS call ([`PropSpec::decode`]), set by the
    /// core.
    Value,
    /// A named slot: JSX in a holder node.
    Slot,
    /// The JSX children.
    Children,
    /// Virtualized rows: in JS `each`, a row renderer and the data props of
    /// `List` (the glue makes the list, the component places it).
    Items,
}

/// What converts a JS value into a prop's Rust type (the prop's
/// [`super::JsValue::from_js`], type-erased): `nsh-js` calls it when JS
/// sets the prop.
pub type PropDecode = fn(JsIn<'_>) -> Result<AnyValue, PropError>;

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
    /// A value prop's value from JS (`None` for slots, children and rows).
    pub decode: Option<PropDecode>,
}

/// One event, for the glue and its declarations.
#[derive(Debug, Clone)]
pub struct EventSpec {
    /// Its name (`press`: the handler is `onPress`).
    pub name: &'static str,
    /// The name of the handler's argument in TypeScript (`value`, or what
    /// `#[args(..)]` on the variant says; unused without data).
    pub arg: &'static str,
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

    /// The props (their named types declared in `types`).
    fn spec(types: &mut TsTypes) -> Vec<PropSpec>;

    /// Every prop at its default.
    fn initial() -> Self;

    /// Prop `name` (its JS name) from JS, as [`PropSpec::decode`] made it;
    /// `None` restores its default. Returns whether it changed.
    fn set(&mut self, name: &str, value: Option<AnyValue>) -> Result<bool, PropError>;

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
    fn spec(_: &mut TsTypes) -> Vec<PropSpec> {
        Vec::new()
    }

    fn initial() -> Self {}

    fn set(&mut self, name: &str, _: Option<AnyValue>) -> Result<bool, PropError> {
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

    /// Its data as a JS value of `host` (converted when JS hears it).
    fn data_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError>;
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

    fn data_js<'a>(&self, _host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match *self {}
    }
}

/// What a component is to JS: its glue and declarations are generated
/// from it ([`declarations`]).
#[derive(Debug, Clone)]
pub struct ComponentSpec {
    /// Its registered name: its type's full path ([`std::any::type_name`]),
    /// unique within one binary.
    pub name: &'static str,
    /// Its JS export: the type's own name, the last segment of its path.
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
            doc: C::DOC,
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

/// A type's own name, its JS export: the last segment of its path, without
/// a generic type's arguments (the test `components_are_named_after_their_types`).
fn export_name(type_name: &str) -> &str {
    let path = type_name.split('<').next().unwrap_or(type_name);
    path.rsplit("::").next().unwrap_or(path)
}

/// An instance: a component's props and view, their types erased.
pub(crate) trait AnyInstance {
    fn set_prop(&mut self, name: &str, value: Option<AnyValue>) -> Result<bool, PropError>;
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
    /// A method of its ref (its arguments decoded); `None` when it has no
    /// ref of the method's type.
    fn call(&mut self, call: MethodCall) -> Option<JsResult>;
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
    appearance: Option<crate::platform::Appearance>,
    app_id: Option<String>,
    status_bar: Option<crate::StatusBarSetting>,
}

impl Root {
    /// Component `C` with the props `props` makes.
    pub fn new<C: Component>(props: impl Fn() -> C::Props + Send + Sync + 'static) -> Self {
        Self {
            make: Arc::new(move || super::IntoElement::into_element(C::element(props()))),
            theme: None,
            fonts: Vec::new(),
            appearance: None,
            app_id: None,
            status_bar: None,
        }
    }

    /// The app's id (`nsh.json` `"id"`, which nsh's generated code sets):
    /// it names where the app keeps its data (`nsh/storage`).
    pub fn app_id(mut self, id: impl Into<String>) -> Self {
        self.app_id = Some(id.into());
        self
    }

    /// The app's id ([`Root::app_id`]).
    pub fn app_id_ref(&self) -> Option<&str> {
        self.app_id.as_deref()
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

    /// Shows the app in `appearance` whatever the system's (`nsh.json`
    /// `"appearance"`; later [`super::ambient::set_color_scheme`] calls
    /// change it).
    pub fn appearance(mut self, appearance: crate::platform::Appearance) -> Self {
        self.appearance = Some(appearance);
        self
    }

    /// The status bar's default (`nsh.json` `"statusBar"`), under what
    /// pages and dialogs set (`Screen::status_bar`, `Modal::status_bar`).
    pub fn status_bar(mut self, setting: crate::StatusBarSetting) -> Self {
        self.status_bar = Some(setting);
        self
    }

    /// The status bar's default ([`Root::status_bar`]).
    pub fn status_bar_ref(&self) -> Option<crate::StatusBarSetting> {
        self.status_bar
    }

    /// The appearance it starts in ([`Root::appearance`]; `None`: the
    /// system's).
    pub fn appearance_ref(&self) -> Option<crate::platform::Appearance> {
        self.appearance
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
    /// The type that implements it.
    type_name: &'static str,
}

#[derive(Clone)]
struct ClassType {
    spec: Arc<ClassSpec>,
    /// The type that implements it.
    type_name: &'static str,
}

/// What a UI set exports to JS: its components and modules, and what it
/// passes on from the runtime. Cheap to clone.
#[derive(Clone, Default)]
pub struct Registry {
    types: Arc<FxHashMap<&'static str, ComponentType>>,
    modules: Arc<FxHashMap<&'static str, ModuleType>>,
    /// By the specifier of their set and their name: the sets merged for
    /// the core may have classes of the same name.
    classes: Arc<FxHashMap<(Option<&'static str>, &'static str), ClassType>>,
    /// Values and types of `nsh/_core` exported as they are.
    reexports: Arc<(Vec<&'static str>, Vec<&'static str>)>,
    /// Values `nsh-js` makes for the set (declared by hand, not generated
    /// from a spec).
    native_values: Arc<Vec<&'static str>>,
    /// The runtime module the functions belong to (`nsh/android`): JS
    /// calls them as `nsh/android:Toast.show`, apart from any UI set's.
    namespace: Option<&'static str>,
    /// The JS module a UI set is imported as (`app/native`).
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
        if self.classes.keys().any(|(_, name)| *name == spec.export) {
            panic!(
                "component `{}` is exported as `{}` like a class{}: rename one",
                spec.name,
                spec.export,
                of_set(self.ui_set.or(self.namespace)),
            );
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
        let mut refs = Vec::new();
        (C::METHODS.refs)(&mut refs);
        self.classes_named(refs)
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
        if self.classes.keys().any(|(_, name)| *name == M::NAME) {
            panic!(
                "module `{type_name}` is named `{}` like a class{}: rename one",
                M::NAME,
                of_set(self.ui_set.or(self.namespace))
            );
        }
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
                type_name,
            },
        );
        let mut refs = Vec::new();
        for f in M::FUNCTIONS {
            (f.refs)(&mut refs);
        }
        self.classes_named(refs)
    }

    /// Registers class `C` (`#[class]`): a JS class named [`Class::DEF`]'s
    /// name, which JS makes with `new` (when it has a constructor) and gets
    /// from the members that return one. The classes its members'
    /// signatures name are registered with it, as are those a module's
    /// functions or a component's methods name ([`Registry::module`],
    /// [`Registry::register`]): their objects reach JS, so they are
    /// exported and declared. Registering a class again does nothing.
    ///
    /// # Panics
    ///
    /// When the set has another class, a module or a component of that
    /// name already.
    #[track_caller]
    pub fn class<C: Class>(&mut self) -> &mut Self {
        self.class_def(C::DEF)
    }

    /// Registers the classes in `refs` (a signature names them), unless
    /// the set has them.
    #[track_caller]
    fn classes_named(&mut self, refs: Vec<&'static ClassDef>) -> &mut Self {
        for def in refs {
            self.class_def(def);
        }
        self
    }

    #[track_caller]
    fn class_def(&mut self, def: &'static ClassDef) -> &mut Self {
        let type_name = (def.type_name)();
        let set = self.ui_set.or(self.namespace);
        if let Some(old) = self.classes.get(&(set, def.name)) {
            if (old.spec.def.type_id)() == (def.type_id)() {
                return self;
            }
            panic!("{}", same_class(def.name, old.type_name, type_name, set));
        }
        if self.modules.contains_key(def.name)
            || self.types.values().any(|t| t.spec.export == def.name)
        {
            panic!(
                "class `{type_name}` is named `{}` like a module or a component{}: rename one",
                def.name,
                of_set(set)
            );
        }
        Arc::make_mut(&mut self.classes).insert(
            (set, def.name),
            ClassType {
                spec: Arc::new(ClassSpec::of(def)),
                type_name,
            },
        );
        // What its members name, in turn (it is registered: a cycle ends).
        let mut refs = Vec::new();
        for m in def.members {
            (m.refs)(&mut refs);
        }
        self.classes_named(refs)
    }

    /// Exports `values` and `types` of the runtime as they are (accessors
    /// such as `safeArea`, hooks, types such as `Insets`).
    pub fn reexport(&mut self, values: &[&'static str], types: &[&'static str]) -> &mut Self {
        let r = Arc::make_mut(&mut self.reexports);
        r.0.extend_from_slice(values);
        r.1.extend_from_slice(types);
        self
    }

    /// The components of all of `sets`, for making them by name (the
    /// core's view of the sets a host serves). Not a module of its own.
    /// Components of different sets may share an export (JS imports them
    /// from different modules), not a name. Modules and classes are not
    /// in it: their calls carry what they run.
    ///
    /// # Panics
    ///
    /// When two sets register the same component or two types of the same
    /// name.
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
        }
        out
    }

    /// The registry of a UI set apps import as `specifier`
    /// (`app/native`). An app's crate makes its own with
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

    /// Exports `values` `nsh-js` makes for the set (`NavStack`, `NavPage`),
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

    /// The registered classes, by name.
    pub fn class_specs(&self) -> Vec<Arc<ClassSpec>> {
        let mut specs: Vec<_> = self.classes.values().map(|c| c.spec.clone()).collect();
        specs.sort_by_key(|s| s.name);
        specs
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

    /// The exports of the set that TS sees without documentation, by
    /// their TS names (`Gauge`, `Gauge.max`, `Gauge.onChange`,
    /// `Ledger.copy`, `new Account`, `Account.balance`): what to give a
    /// doc comment. A component is documented by its `#[component]`
    /// function, a module or class by its struct, the rest by their own
    /// (a setter needs none beside its getter's; a module merged into a
    /// component of its name, or exported flat, by its component or its
    /// functions).
    pub fn undocumented(&self) -> Vec<String> {
        let mut out = Vec::new();
        let specs = self.specs();
        for c in &specs {
            if c.doc.is_empty() {
                out.push(c.export.clone());
            }
            for p in c.props.iter().filter(|p| p.doc.is_empty()) {
                out.push(format!("{}.{}", c.export, p.name));
            }
            for e in c.events.iter().filter(|e| e.doc.is_empty()) {
                out.push(format!("{}.{}", c.export, handler_name(e.name)));
            }
            for m in c.methods.iter().filter(|m| m.doc.is_empty()) {
                out.push(format!("{}.{}", c.export, m.name));
            }
        }
        for m in self.module_specs() {
            let merged = specs.iter().any(|c| c.export == m.name);
            if m.doc.is_empty() && !merged && !m.flat {
                out.push(m.name.to_owned());
            }
            for f in m.functions.iter().filter(|f| f.doc.is_empty()) {
                out.push(if m.flat {
                    f.name.to_owned()
                } else {
                    format!("{}.{}", m.name, f.name)
                });
            }
        }
        for c in self.class_specs() {
            if c.doc.is_empty() {
                out.push(c.name.to_owned());
            }
            let documented_getter = |name: &str| {
                c.members
                    .iter()
                    .any(|m| m.kind == MemberKind::Getter && m.name == name && !m.doc.is_empty())
            };
            for m in c.members.iter().filter(|m| m.doc.is_empty()) {
                match m.kind {
                    MemberKind::Constructor => out.push(format!("new {}", c.name)),
                    MemberKind::Setter if documented_getter(m.name) => {}
                    _ => out.push(format!("{}.{}", c.name, m.name)),
                }
            }
        }
        out
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

/// The message of two classes named `name`.
fn same_class(name: &str, old: &str, new: &str, set: Option<&str>) -> String {
    if old == new {
        format!("class `{old}` is registered twice{}", of_set(set))
    } else {
        format!(
            "classes `{old}` and `{new}` are both named `{name}`{}: rename one",
            of_set(set)
        )
    }
}

/// ` in `app/native`` for a registry with a specifier.
fn of_set(set: Option<&str>) -> String {
    set.map(|s| format!(" in `{s}`")).unwrap_or_default()
}

/// The declarations of what `registry`'s specs export: one component per
/// spec (by its [`ComponentSpec::export`] name), one object per module
/// (its type `<Name>Module`; merged into a component of the same name; a
/// flat module's functions one by one), one class per class, and the
/// named types they use.
/// Exports named in `omit` are left out (their types stay): `nsh-js`
/// declares them, building on the module (`BackHandlerModule & ...`), as
/// it declares the re-exports and native values it makes.
pub fn declarations(registry: &Registry, omit: &[&str]) -> String {
    let specs = registry.specs();
    let modules = registry.module_specs();
    let classes = registry.class_specs();
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
    for c in &classes {
        types.merge(&c.types);
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
        let value = f.returns.clone().unwrap_or_else(|| "void".to_owned());
        let returns = if f.promises() {
            format!("Promise<{value}>")
        } else {
            value
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
                format!("{}: {}", e.arg, e.data)
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
    for c in classes.iter().filter(|c| !omit.contains(&c.name)) {
        class_declaration(&mut out, c);
    }
    out
}

/// `export declare class Name { ... }`.
fn class_declaration(out: &mut String, c: &ClassSpec) {
    use std::fmt::Write as _;
    let doc = |out: &mut String, indent: &str, doc: &str| {
        if !doc.is_empty() {
            let _ = writeln!(out, "{indent}/** {doc} */");
        }
    };
    doc(out, "", c.doc);
    let _ = writeln!(out, "export declare class {} {{", c.name);
    if !c.members.iter().any(|m| m.kind == MemberKind::Constructor) {
        out.push_str("  /** Made by the class's members only. */\n  private constructor();\n");
    }
    for m in &c.members {
        doc(out, "  ", m.doc);
        let params = m
            .params
            .iter()
            .map(|(n, ts, optional)| format!("{n}{}: {ts}", if *optional { "?" } else { "" }))
            .collect::<Vec<_>>()
            .join(", ");
        let value = m.returns.clone().unwrap_or_else(|| "void".to_owned());
        let returns = if m.is_async {
            format!("Promise<{value}>")
        } else {
            value
        };
        let _ = match m.kind {
            MemberKind::Constructor => writeln!(out, "  constructor({params});"),
            MemberKind::Method => writeln!(out, "  {}({params}): {returns};", m.name),
            MemberKind::Getter => writeln!(out, "  get {}(): {returns};", m.name),
            MemberKind::Setter => writeln!(out, "  set {}({params});", m.name),
            MemberKind::Static => writeln!(out, "  static {}({params}): {returns};", m.name),
        };
    }
    out.push_str(
        "  /** Frees the Rust value now (else when the object is collected); the object throws after. */\n  dispose(): void;\n  [Symbol.dispose](): void;\n}\n\n",
    );
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
        #[args(limit)]
        Limit(u32),
        Reset,
    }

    struct StepperApi;

    #[methods]
    impl StepperApi {
        /// Jumps to `value`.
        pub(crate) fn jump_to(&self, value: f64) {
            let _ = value;
        }

        pub(crate) fn value(&self) -> f64 {
            0.0
        }

        /// A helper without `pub`: not a method of the ref.
        #[allow(dead_code)]
        fn clamp(&self, value: f64) -> f64 {
            value
        }
    }

    impl Component for Stepper {
        type Props = StepperProps;
        type Event = StepperEvent;
        const METHODS: Methods = Methods::of::<StepperApi>();
        const DOC: &'static str = "Picks a number.";

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
            "  /** The value changed. */\n  onChange?: (event: { newValue: number }) => void;",
            "  onLimit?: (limit: number) => void;",
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
        assert!(!ts.contains("clamp"), "a helper is not a method:\n{ts}");
        assert_eq!(StepperApi::METHODS.len(), 2);
        let mut props = StepperProps::initial();
        let size = |json| {
            let specs = StepperProps::spec(&mut TsTypes::default());
            let decode = specs
                .iter()
                .find(|p| p.name == "size")
                .unwrap()
                .decode
                .unwrap();
            super::super::json_host::decode(decode, json)
        };
        assert!(
            props
                .set("size", Some(size(serde_json::json!("extraLarge")).unwrap()))
                .unwrap()
        );
        assert!(props.size == StepperSize::ExtraLarge);
        // A value of another type: the conversion fails (JS warns, the prop
        // keeps its value), and so does a decoded value of another prop's
        // type.
        assert!(size(serde_json::json!("huge")).is_err());
        assert!(props.set("size", Some(AnyValue::new(1.0f64))).is_err());
        assert!(props.set("size", None).unwrap());
        assert!(props.size == StepperSize::Regular);
        let data = |e: &StepperEvent| super::super::json_host::event_data(e);
        assert_eq!(
            data(&StepperEvent::Change { new_value: 2.5 }),
            Some(serde_json::json!({ "newValue": 2.5 }))
        );
        assert_eq!(data(&StepperEvent::Limit(3)), Some(serde_json::json!(3)));
        assert_eq!(data(&StepperEvent::Reset), None);
        assert_eq!(StepperEvent::Reset.name(), "reset");
    }

    #[test]
    fn exports_without_docs_are_listed() {
        let mut registry = Registry::new();
        registry.register::<Stepper>();
        // `size`, `maxValue`, `children`, `onLimit`, `onReset` and the
        // ref's `value` have no doc comment.
        assert_eq!(
            registry.undocumented(),
            [
                "Stepper.size",
                "Stepper.maxValue",
                "Stepper.children",
                "Stepper.onLimit",
                "Stepper.onReset",
                "Stepper.value",
            ]
        );
        let mut r = Registry::ui_set("test/maker");
        r.module::<Maker>();
        assert_eq!(r.undocumented(), ["Maker.make", "new Lonely"]);
    }

    /// A class no set registers.
    #[crate::ui::class]
    struct Lonely;

    #[crate::ui::class]
    impl Lonely {
        pub(crate) fn new() -> Self {
            Lonely
        }
    }

    /// Makes a `Lonely`.
    #[crate::ui::module]
    struct Maker;

    #[crate::ui::module]
    impl Maker {
        pub(crate) fn make() -> Instance<Lonely> {
            Instance::new(Lonely)
        }
    }

    #[test]
    fn the_classes_a_signature_names_are_registered_with_it() {
        let mut r = Registry::ui_set("test/maker");
        r.module::<Maker>();
        // The module's result names `Lonely`: it is the set's, declared and
        // exported.
        let names: Vec<_> = r.class_specs().iter().map(|c| c.name).collect();
        assert_eq!(names, ["Lonely"]);
        let ts = declarations(&r, &[]);
        assert!(ts.contains("export declare class Lonely"), "{ts}");
        assert!(ts.contains("make(): Lonely;"), "{ts}");
        // Registering it again does nothing.
        r.class::<Lonely>();
        assert_eq!(r.class_specs().len(), 1);
        let _ = Registry::merged(&[r]);
    }

    mod clash {
        /// Another class named `Lonely`.
        #[crate::ui::class]
        pub(super) struct Lonely;

        #[crate::ui::class]
        impl Lonely {
            pub(super) fn new() -> Self {
                Lonely
            }
        }
    }

    #[test]
    fn a_clash_through_a_signature_names_both_types() {
        let mut r = Registry::ui_set("test/maker");
        r.class::<clash::Lonely>();
        // `Maker.make` names the other `Lonely`.
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            r.module::<Maker>();
        }))
        .unwrap_err();
        let message = failed.downcast_ref::<String>().unwrap();
        assert!(
            message.contains("`nsh_core::ui::component::tests::clash::Lonely`")
                && message.contains("`nsh_core::ui::component::tests::Lonely`"),
            "{message}"
        );
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
