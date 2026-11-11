// What the host app answers (fake data: the real one asks Longbridge's
// search APIs). Each search resolves after a short pause, as a request does.

/** One candidate of a search: `id` is what the server stores. */
export interface Candidate {
  id: string;
  label: string;
  detail: string;
}

const SECURITIES: Candidate[] = [
  { id: "ST/US/TSLA", label: "Tesla", detail: "TSLA.US" },
  { id: "ST/US/NVDA", label: "NVIDIA", detail: "NVDA.US" },
  { id: "ST/US/AAPL", label: "Apple", detail: "AAPL.US" },
  { id: "ST/HK/700", label: "腾讯控股", detail: "700.HK" },
  { id: "ST/HK/9988", label: "阿里巴巴-W", detail: "9988.HK" },
  { id: "ST/HK/1810", label: "小米集团-W", detail: "1810.HK" },
  { id: "ST/SH/600519", label: "贵州茅台", detail: "600519.SH" },
];

const MEMBERS: Candidate[] = [
  { id: "1001", label: "Alice", detail: "分析师" },
  { id: "1002", label: "Bob", detail: "基金经理" },
  { id: "1003", label: "老王", detail: "港股老兵" },
  { id: "1004", label: "小陈", detail: "美股研究" },
  { id: "1005", label: "Grace", detail: "宏观" },
];

const TOPICS: Candidate[] = [
  { id: "t1", label: "财报季", detail: "12.3 万讨论" },
  { id: "t2", label: "Earnings", detail: "8.1 万讨论" },
  { id: "t3", label: "AI 算力", detail: "6.4 万讨论" },
  { id: "t4", label: "降息", detail: "5.2 万讨论" },
  { id: "t5", label: "新能源车", detail: "3.9 万讨论" },
];

/** The candidates of `pool` whose label, detail or id holds `query`; all of them for an empty one. */
function search(pool: Candidate[], query: string): Promise<Candidate[]> {
  const q = query.trim().toLowerCase();
  const found = pool.filter((c) => !q || `${c.label} ${c.detail} ${c.id}`.toLowerCase().includes(q));
  return new Promise((resolve) => setTimeout(() => resolve(found), 40));
}

/** The host's interface to the editor. */
export const host = {
  searchSecurities: (query: string) => search(SECURITIES, query),
  searchMembers: (query: string) => search(MEMBERS, query),
  searchTopics: (query: string) => search(TOPICS, query),
};
