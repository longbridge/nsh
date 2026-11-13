// A small site whose pages search engines can index: each page of the stack
// is a URL (`UrlScheme.url()` / `setUrl`), headings, paragraphs and links are
// HTML elements (`role="header"`, `Link`'s `href`, `web={{ element }}`), and
// each page sets the document's head (`nsh/web`'s `Head`). Its notes and
// prices come from the site's API. `build.mjs` writes each page's first screen
// as static HTML when the site is built (`nsh render`), with the responses it
// was written with, which the app starts from (`host.params`) before it reads
// the API again.
import { host, mount, Scroll, signal, Text, UrlScheme, View, type Accessor } from "nsh";
import { createNavStack, Link, NavPage, NavStack } from "nsh/base";
import { Head } from "nsh/web";

interface NoteSummary {
  slug: string;
  symbol: string;
  date: string;
  title: string;
  summary: string;
}

interface Note extends NoteSummary {
  body: string[];
}

interface Quote {
  symbol: string;
  price: number;
  change: number;
  at: string;
}

type Route = { name: "home" } | { name: "note"; slug: string };

/** The path of the page showing `route`. */
const pathOf = (route: Route) => (route.name === "note" ? `/notes/${route.slug}` : "/");

/** The routes the address `url` opens, bottom to top: an unknown path opens
 * the home page. */
function routesOf(url: string | null): Route[] {
  const path = url ? new URL(url, "https://site.invalid/").pathname : "/";
  const match = /^\/notes\/([\w-]+)\/?$/.exec(path);
  return match ? [{ name: "home" }, { name: "note", slug: match[1] }] : [{ name: "home" }];
}

// The page's address opens its pages: in the browser, and in `nsh render`,
// which starts the app at the address it renders.
const nav = createNavStack<Route>(routesOf(UrlScheme.url()));
// A link, and the browser's forward to a page the stack no longer has.
UrlScheme.on("open", ({ url }) => nav.setRoutes(routesOf(url)));

const SITE = "Market Notes";

/** The API's responses the written page was rendered with, by path. */
const written = ((host.params as { fetches?: Record<string, unknown> }).fetches ?? {}) as Record<string, unknown>;

/** `path`'s JSON: what the written page had at once, then the API's. */
function load<T>(path: string): Accessor<T | undefined> {
  const value = signal(written[path] as T | undefined);
  fetch(path)
    .then((r) => (r.ok ? r.json() : undefined))
    .then((v) => v !== undefined && value.set(v as T))
    .catch(() => {});
  return value;
}

/** The latest quotes of `symbols`, asked for every two seconds. */
function quotes(symbols: string[]): Accessor<Quote[]> {
  const path = `/api/quotes?symbols=${symbols.join(",")}`;
  const value = signal((written[path] as Quote[] | undefined) ?? []);
  const poll = () =>
    fetch(path)
      .then((r) => r.json())
      .then((v) => value.set(v as Quote[]))
      .catch(() => {});
  poll();
  setInterval(poll, 2000);
  return value;
}

const notes = load<NoteSummary[]>("/api/notes");

const time = (iso: string) => new Date(iso).toISOString().slice(11, 19);

function SiteHeader() {
  return (
    <View web={{ element: "header" }} class="h-14 flex-row items-center justify-between border-b border-border bg-background px-6">
      <Link href="/" openWith="app" onOpen={() => nav.popToRoot()} class="flex-row items-center gap-2 active:opacity-60">
        <View class="h-6 w-6 rounded-md bg-primary" />
        <Text class="text-base font-semibold text-foreground">{SITE}</Text>
      </Link>
      <View class="flex-row items-center gap-5">
        <View web={{ element: "nav" }} class="flex-row gap-5">
          <Text class="text-sm text-muted-foreground">Research</Text>
          <Text class="text-sm text-muted-foreground">About</Text>
        </View>
      </View>
    </View>
  );
}

function Price(props: { quote: Quote | undefined }) {
  return (
    <If when={props.quote}>
      {(quote) => (
        <View class="flex-row items-center gap-2">
          <Text class="text-sm font-medium text-foreground" style={{ fontVariant: ["tabular-nums"] }}>
            {quote().price.toFixed(2)}
          </Text>
          <Text
            class={quote().change >= 0 ? "text-sm text-up" : "text-sm text-down"}
            style={{ fontVariant: ["tabular-nums"] }}
          >
            {`${quote().change >= 0 ? "+" : ""}${quote().change.toFixed(2)}%`}
          </Text>
          <Text class="text-xs text-muted-foreground">{`as of ${time(quote().at)}`}</Text>
        </View>
      )}
    </If>
  );
}

function NoteCard(props: { note: NoteSummary; quote: Quote | undefined }) {
  return (
    <Link
      href={pathOf({ name: "note", slug: props.note.slug })}
      openWith="app"
      onOpen={() => nav.push({ name: "note", slug: props.note.slug })}
      class="gap-2 rounded-xl border border-border bg-card p-5 active:bg-muted"
    >
      <View class="flex-row flex-wrap items-center justify-between gap-y-1">
        <View class="flex-row items-center gap-2">
          <Text class="rounded-md bg-muted px-2 py-0.5 text-xs font-medium text-foreground">{props.note.symbol}</Text>
          <Text web={{ element: "time" }} class="text-xs text-muted-foreground">{props.note.date}</Text>
        </View>
        <Price quote={props.quote} />
      </View>
      <Text role="header" accessibilityLevel={2} class="text-lg font-semibold text-card-foreground">{props.note.title}</Text>
      <Text web={{ element: "p" }} class="text-sm leading-5 text-muted-foreground">{props.note.summary}</Text>
    </Link>
  );
}

function Home() {
  const prices = quotes(["AAPL", "NVDA", "700.HK"]);
  const quoteOf = (symbol: string) => prices().find((q) => q.symbol === symbol);
  return (
    <NavPage title={SITE} header={<SiteHeader />}>
      <Head
        title={`${SITE} · Notes on the markets`}
        description="Short research notes on the companies moving the markets: earnings, products and what to watch next."
        canonical="/"
        lang="en"
        jsonLd={{ "@context": "https://schema.org", "@type": "WebSite", name: SITE }}
      />
      <Scroll class="flex-1 bg-background">
        <View web={{ element: "main" }} class="w-full max-w-3xl self-center gap-6 px-6 py-10">
          <View class="gap-2">
            <Text role="header" accessibilityLevel={1} class="text-3xl font-bold text-foreground">Notes on the markets</Text>
            <Text web={{ element: "p" }} class="text-base leading-6 text-muted-foreground">
              Short research notes on the companies moving the markets: earnings, products and what to watch next.
            </Text>
          </View>
          <View web={{ element: "section" }} class="gap-4">
            <Each of={notes() ?? []} key={(note) => note.slug}>{(note) => <NoteCard note={note()} quote={quoteOf(note().symbol)} />}</Each>
          </View>
        </View>
      </Scroll>
    </NavPage>
  );
}

function NotePage(props: { slug: string }) {
  const note = load<Note>(`/api/notes/${props.slug}`);
  return (
    <NavPage title={note()?.title ?? SITE} header={<SiteHeader />}>
      <If when={note()}>
        {(note) => (
          <Head
            title={`${note().title} · ${SITE}`}
            description={note().summary}
            canonical={`/notes/${note().slug}`}
            meta={[{ property: "og:type", content: "article" }]}
            jsonLd={{
              "@context": "https://schema.org",
              "@type": "Article",
              headline: note().title,
              datePublished: note().date,
              about: note().symbol,
            }}
          />
        )}
      </If>
      <Scroll class="flex-1 bg-background">
        <View web={{ element: "article" }} class="w-full max-w-2xl self-center gap-5 px-6 py-10">
          <Link href="/" openWith="app" onOpen={() => nav.pop()} class="flex-row active:opacity-60">
            <Text class="text-sm text-muted-foreground">← All notes</Text>
          </Link>
          <If when={note()}>
            {(note) => (
              <View class="gap-5">
                <View class="flex-row items-center gap-2">
                  <Text class="rounded-md bg-muted px-2 py-0.5 text-xs font-medium text-foreground">{note().symbol}</Text>
                  <Text web={{ element: "time" }} class="text-xs text-muted-foreground">{note().date}</Text>
                </View>
                <Text role="header" accessibilityLevel={1} class="text-3xl font-bold leading-10 text-foreground">{note().title}</Text>
                <Each of={note().body}>
                  {(paragraph) => <Text web={{ element: "p" }} class="text-base leading-7 text-foreground">{paragraph()}</Text>}
                </Each>
              </View>
            )}
          </If>
        </View>
      </Scroll>
    </NavPage>
  );
}

function App() {
  return (
    <NavStack
      state={nav}
      onChange={({ routes }) => UrlScheme.setUrl(pathOf(routes[routes.length - 1]))}
      class="flex-1"
    >
      {(route) => (route.name === "note" ? <NotePage slug={route.slug} /> : <Home />)}
    </NavStack>
  );
}

mount(App);
