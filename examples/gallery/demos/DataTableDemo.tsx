import { signal, Text, View } from "nsh";
import {
  Button,
  DataTable,
  type DataTableColumn,
  type DataTableRef,
  type DataTableSelection,
  type DataTableSort,
} from "nsh/base";
import { Segmented } from "../ui";

// `nsh/base`'s `DataTable` over a million rows of sixty columns the app does
// not hold: the table asks for the rows in view (`onVisibleRowsChange`) and
// the app answers with a page of them around (`data`, `dataStart`,
// `rowCount`), so the table scrolls as a small one does. Sorting is the
// app's: it reports the next sort (`onSortChange`), the app orders what it
// serves and sets `sort`.

const ROWS = 1_000_000;
const METRICS = 56;
const PAGE = 400;
const NAMES = ["Ada", "Alan", "Grace", "Linus", "Margaret", "Dennis", "Barbara", "Ken"];
const CITIES = ["Oslo", "Lima", "Kyoto", "Cairo", "Perth", "Quito", "Turin", "Hanoi"];

const columns: DataTableColumn[] = [
  { key: "id", title: "ID", width: 90, align: "end", fixed: true, sortable: true },
  { key: "name", title: "Name", width: 150, sortable: true },
  { key: "city", title: "City", width: 130 },
  { key: "score", title: "Score", width: 100, align: "end", sortable: true },
  ...Array.from({ length: METRICS }, (_, i) => ({
    key: `m${i + 1}`,
    title: `Metric ${i + 1}`,
    width: 110,
    align: "end" as const,
  })),
];

/** The row at position `at` of the table, with the order `descending` gives. */
function rowAt(at: number, descending: boolean): (string | number)[] {
  const id = descending ? ROWS - 1 - at : at;
  const row: (string | number)[] = [
    id + 1,
    `${NAMES[id % NAMES.length]} ${id}`,
    CITIES[(id * 7) % CITIES.length],
    (id * 37) % 1000,
  ];
  for (let m = 0; m < METRICS; m++) row.push(((id + 1) * (m + 3)) % 10_000);
  return row;
}

type Page = { start: number; rows: (string | number)[][] };

function load(from: number, descending: boolean): Page {
  const start = Math.max(0, Math.min(ROWS - PAGE, from));
  return { start, rows: Array.from({ length: PAGE }, (_, i) => rowAt(start + i, descending)) };
}

/** A small button in the gallery's look. */
function Tool(props: { label: string; name: string; onPress: () => void }) {
  return (
    <Button
      onPress={props.onPress}
      accessibilityLabel={props.label}
      class="h-8 items-center justify-center rounded-md border border-border bg-card px-3 active:opacity-70"
    >
      <Text class="text-[13px] font-semibold text-foreground">{props.name}</Text>
    </Button>
  );
}

/**
 * Data table: a million rows and sixty columns, the first four columns
 * sortable and the ID column fixed at the left, rows, cells or columns
 * selected, columns resized and moved, the rows in view fetched a page at a
 * time.
 */
export function DataTableDemo() {
  const mode = signal<"row" | "cell" | "column">("row");
  const sort = signal<DataTableSort | undefined>(undefined);
  const page = signal<Page>(load(0, false));
  const selection = signal<DataTableSelection | null>(null);
  const visible = signal({ start: 0, end: 0 });
  const stripe = signal(true);
  let table: DataTableRef | undefined;

  const descending = () => sort()?.key === "id" && sort()?.direction === "descending";

  // The rows in view stay inside the page held: when they come near its
  // edge, a page around them is served.
  const fetchAround = (start: number, end: number) => {
    const held = page();
    const margin = 60;
    if (start >= held.start + margin && end <= held.start + PAGE - margin) return;
    if (held.start === 0 && start < PAGE - margin && end <= PAGE - margin) return;
    page.set(load(start - (PAGE - (end - start)) / 2, descending()));
  };

  const describe = () => {
    const s = selection();
    if (!s) return "Nothing selected";
    if (s.row !== undefined && s.column !== undefined) return `Cell row ${s.row + 1}, ${columns[s.column].title}`;
    if (s.row !== undefined) return `Row ${s.row + 1}`;
    return `Column ${columns[s.column!].title}`;
  };

  return (
    <View class="flex-1 gap-3 bg-background p-4">
      <View class="flex-row flex-wrap items-center gap-3">
        <View class="w-[260px]">
          <Segmented
            options={[
              { value: "row", label: "Rows" },
              { value: "cell", label: "Cells" },
              { value: "column", label: "Columns" },
            ]}
            value={mode()}
            onChange={(value) => {
              mode.set(value);
              selection.set(null);
            }}
            size="small"
          />
        </View>
        <Tool label="Toggle stripes" name={stripe() ? "Stripes on" : "Stripes off"} onPress={() => stripe.set(!stripe())} />
        <Tool label="Go to row 500000" name="Row 500,000" onPress={() => table?.scrollToRow(499_999)} />
        <Tool label="Go to Metric 40" name="Metric 40" onPress={() => table?.scrollToColumn("m40")} />
      </View>
      <View class="flex-row items-center gap-3">
        <Text class="font-mono text-[12px] text-foreground">{describe()}</Text>
        <Text class="font-mono text-[12px] text-muted-foreground">
          {`rows ${visible().start + 1}–${visible().end} of ${ROWS.toLocaleString("en-US")}`}
        </Text>
      </View>
      <DataTable
        ref={(r) => (table = r)}
        testID="table"
        class="flex-1"
        columns={columns}
        data={page().rows}
        dataStart={page().start}
        rowCount={ROWS}
        rowHeader
        stripe={stripe()}
        selectionMode={mode()}
        sort={sort()}
        onSortChange={(next) => {
          sort.set(next ?? undefined);
          page.set(load(visible().start - 100, next?.key === "id" && next.direction === "descending"));
        }}
        onSelectionChange={(next) => selection.set(next)}
        onVisibleRowsChange={(range) => {
          visible.set(range);
          fetchAround(range.start, range.end);
        }}
      />
    </View>
  );
}
