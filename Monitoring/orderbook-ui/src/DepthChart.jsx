// src/DepthChart.jsx

import {
  Area,
  AreaChart,
  CartesianGrid,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

function buildDepthData(bids, asks) {
  const sortedBids = [...bids].sort((a, b) => b.price - a.price);
  const sortedAsks = [...asks].sort((a, b) => a.price - b.price);

  let bidRunning = 0;
  const bidData = sortedBids.map((level) => {
    bidRunning += level.quantity;
    return {
      price: level.price,
      bidDepth: bidRunning,
      askDepth: null,
    };
  });

  let askRunning = 0;
  const askData = sortedAsks.map((level) => {
    askRunning += level.quantity;
    return {
      price: level.price,
      bidDepth: null,
      askDepth: askRunning,
    };
  });

  const merged = [...bidData, ...askData].sort((a, b) => a.price - b.price);

  return { merged, sortedBids, sortedAsks };
}

export default function DepthChart({ bids, asks }) {
  const { merged, sortedBids, sortedAsks } = buildDepthData(bids, asks);

  const bestBid = sortedBids.length ? sortedBids[0].price : null;
  const bestAsk = sortedAsks.length ? sortedAsks[0].price : null;
  const mid =
    bestBid != null && bestAsk != null ? (bestBid + bestAsk) / 2 : null;

  const minX =
    merged.length > 0
      ? Math.min(...merged.map((d) => d.price), (bestBid ?? 0) - 1)
      : 0;

  const maxX =
    merged.length > 0
      ? Math.max(...merged.map((d) => d.price), (bestAsk ?? 10) + 1)
      : 10;

  return (
    <div className="chart-wrap">
      <ResponsiveContainer width="100%" height={360}>
        <AreaChart data={merged} margin={{ top: 10, right: 10, left: -20, bottom: 0 }}>
          <defs>
            <linearGradient id="bidFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#22c55e" stopOpacity={0.35} />
              <stop offset="100%" stopColor="#22c55e" stopOpacity={0.02} />
            </linearGradient>
            <linearGradient id="askFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#ef4444" stopOpacity={0.35} />
              <stop offset="100%" stopColor="#ef4444" stopOpacity={0.02} />
            </linearGradient>
          </defs>

          <CartesianGrid stroke="rgba(255,255,255,0.08)" strokeDasharray="4 4" />
          <XAxis
            dataKey="price"
            type="number"
            domain={[minX, maxX]}
            tick={{ fill: "rgba(255,255,255,0.58)", fontSize: 12 }}
            axisLine={{ stroke: "rgba(255,255,255,0.10)" }}
            tickLine={{ stroke: "rgba(255,255,255,0.10)" }}
          />
          <YAxis
            tick={{ fill: "rgba(255,255,255,0.58)", fontSize: 12 }}
            axisLine={{ stroke: "rgba(255,255,255,0.10)" }}
            tickLine={{ stroke: "rgba(255,255,255,0.10)" }}
          />
          <Tooltip
            contentStyle={{
              background: "rgba(10,14,25,0.96)",
              border: "1px solid rgba(255,255,255,0.08)",
              borderRadius: "14px",
              color: "#fff",
              boxShadow: "0 12px 32px rgba(0,0,0,0.35)",
            }}
          />

          {bestBid != null && (
            <ReferenceLine x={bestBid} stroke="rgba(34,197,94,0.55)" strokeDasharray="3 3" />
          )}
          {bestAsk != null && (
            <ReferenceLine x={bestAsk} stroke="rgba(239,68,68,0.55)" strokeDasharray="3 3" />
          )}
          {mid != null && (
            <ReferenceLine x={mid} stroke="rgba(168,85,247,0.7)" strokeDasharray="6 6" />
          )}

          <Area
            type="stepAfter"
            dataKey="bidDepth"
            stroke="#22c55e"
            fill="url(#bidFill)"
            strokeWidth={2.25}
            isAnimationActive={false}
            connectNulls={false}
          />
          <Area
            type="stepBefore"
            dataKey="askDepth"
            stroke="#ef4444"
            fill="url(#askFill)"
            strokeWidth={2.25}
            isAnimationActive={false}
            connectNulls={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}