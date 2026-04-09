// src/DepthChart.jsx

import {
  ResponsiveContainer,
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  CartesianGrid,
} from "recharts";

export default function DepthChart({ bids, asks }) {
  const sortedBids = [...bids].sort((a, b) => b.price - a.price);
  const sortedAsks = [...asks].sort((a, b) => a.price - b.price);

  let bidTotal = 0;
  const bidData = sortedBids.map((b) => {
    bidTotal += b.quantity;
    return {
      price: b.price,
      bidDepth: bidTotal,
      askDepth: null,
    };
  });

  let askTotal = 0;
  const askData = sortedAsks.map((a) => {
    askTotal += a.quantity;
    return {
      price: a.price,
      bidDepth: null,
      askDepth: askTotal,
    };
  });

  const data = [...bidData, ...askData].sort((a, b) => a.price - b.price);

  const bestBid = sortedBids.length ? sortedBids[0].price : 0;
  const bestAsk = sortedAsks.length ? sortedAsks[0].price : bestBid + 1;

  const minX =
    data.length > 0 ? Math.min(...data.map((d) => d.price), bestBid - 1) : 0;
  const maxX =
    data.length > 0 ? Math.max(...data.map((d) => d.price), bestAsk + 1) : 10;

  return (
    <div style={{ width: "100%", height: 320 }}>
      <ResponsiveContainer>
        <LineChart data={data} margin={{ top: 10, right: 20, left: 10, bottom: 10 }}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis type="number" dataKey="price" domain={[minX, maxX]} />
          <YAxis />
          <Tooltip />
          <Line
            type="stepAfter"
            dataKey="bidDepth"
            stroke="#00ff00"
            dot={false}
            connectNulls={false}
            isAnimationActive={false}
          />
          <Line
            type="stepBefore"
            dataKey="askDepth"
            stroke="#ff0000"
            dot={false}
            connectNulls={false}
            isAnimationActive={false}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}