// The demo's content: what a CMS or a research database would hold. The
// API reads it. Its amounts (`$26 billion`) and names in other scripts
// (腾讯) go through the build as any text does.
export const notes = [
  {
    slug: "apple-services-margin",
    symbol: "AAPL",
    date: "2026-10-31",
    title: "Apple's services keep lifting its margins",
    summary: "Services passed a third of revenue this quarter, and their margin is twice the hardware's.",
    body: [
      "Apple's services business grew faster than its hardware again this quarter: at more than $26 billion, it now brings in over a third of the company's revenue.",
      "Because services earn roughly twice the gross margin of devices, every point of mix they gain lifts the company's overall margin, even in a quarter when iPhone sales are flat.",
      "What to watch next: subscription renewals after the price changes, and whether regulators in Europe and the US reshape the App Store's commissions.",
    ],
  },
  {
    slug: "nvidia-data-center-demand",
    symbol: "NVDA",
    date: "2026-10-24",
    title: "NVIDIA's data center demand shows no sign of slowing",
    summary: "Orders from cloud providers outran supply again, and the next generation of chips is already booked.",
    body: [
      "NVIDIA's data center segment set another record, with the largest cloud providers still buying every accelerator the company can ship.",
      "Supply, not demand, remains the limit: packaging capacity at its manufacturing partners decides how much revenue each quarter can hold.",
      "What to watch next: the ramp of the next architecture, and how much of the spending shifts from training to running models in production.",
    ],
  },
  {
    slug: "tencent-games-and-ads",
    symbol: "700.HK",
    date: "2026-10-17",
    title: "Tencent's games and ads both return to growth",
    summary: "New titles at home and steady overseas sales lifted games, while video ads gained from better targeting.",
    body: [
      "Tencent (腾讯)'s domestic games grew on the strength of two new titles, and its international studios kept their sales steady after a strong year.",
      "Advertising grew faster than the market, helped by video accounts and by models that match ads to viewers more precisely.",
      "What to watch next: approvals of new game licenses, and how quickly mini programs turn their traffic into transaction fees.",
    ],
  },
];

/** Where each symbol's made-up price starts. */
export const prices = { AAPL: 268.4, NVDA: 182.1, "700.HK": 641.5 };
