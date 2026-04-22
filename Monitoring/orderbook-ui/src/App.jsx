// src/App.jsx

import { useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import DepthChart from "./DepthChart";
import MidPriceChart from "./MidPriceChart";

const MAX_EVENTS = 200;
const MAX_MID_HISTORY = 120;
const ROLLING_WINDOW_MS = 10_000;

function formatNumber(value, digits = 0) {
  if (value == null || Number.isNaN(value)) return "--";
  return Number(value).toLocaleString(undefined, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
}

function formatTime(ts) {
  return new Date(ts).toLocaleTimeString();
}

function getEventMeta(event) {
  if (!event || typeof event !== "object") {
    return {
      label: "Unknown",
      tone: "neutral",
      summary: String(event),
    };
  }

  if (event.OrderAccepted != null) {
    return {
      label: "Order Accepted",
      tone: "success",
      summary: `Accepted order #${event.OrderAccepted}`,
    };
  }

  if (event.OrderRemoved != null) {
    return {
      label: "Order Removed",
      tone: "muted",
      summary: `Removed order #${event.OrderRemoved}`,
    };
  }

  if (event.CancellationFailure != null) {
    return {
      label: "Cancel Failed",
      tone: "danger",
      summary: `Cancellation failed: ${JSON.stringify(event.CancellationFailure)}`,
    };
  }

  if (event.OrderRejected != null) {
    return {
      label: "Order Rejected",
      tone: "danger",
      summary: `Rejected: ${JSON.stringify(event.OrderRejected)}`,
    };
  }

  if (event.TradeExecuted) {
    const trade = event.TradeExecuted;
    return {
      label: "Trade",
      tone: "trade",
      summary: `Trade ${trade.quantity ?? "?"} @ ${trade.price ?? "?"} | taker ${
        trade.taker_id ?? "?"
      } | maker ${trade.maker_id ?? "?"}`,
    };
  }

  const key = Object.keys(event)[0];
  return {
    label: key || "Event",
    tone: "neutral",
    summary: JSON.stringify(event),
  };
}

export default function App() {
  const [connected, setConnected] = useState(false);
  const [events, setEvents] = useState([]);
  const [orderbook, setOrderbook] = useState({ bids: [], asks: [] });
  const [midHistory, setMidHistory] = useState([]);
  const [snapshotCount, setSnapshotCount] = useState(0);
  const [lastSnapshotAt, setLastSnapshotAt] = useState(null);
  const wsRef = useRef(null);

  useEffect(() => {
    const ws = new WebSocket("ws://127.0.0.1:9001");
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      ws.send("subscribe");
    };

    ws.onclose = () => {
      setConnected(false);
    };

    ws.onerror = (err) => {
      console.error("WebSocket error:", err);
    };

    ws.onmessage = (msg) => {
      const parsed = JSON.parse(msg.data);
      const now = Date.now();

      if (parsed.type === "snapshot") {
        const bids = parsed.data?.bid_infos || [];
        const asks = parsed.data?.ask_infos || [];

        setOrderbook((prev) => {
          const blend = (oldLevels, newLevels) => {
          const oldMap = new Map(oldLevels.map(l => [l.price, l.quantity]));

          return newLevels.map(l => ({
            price: l.price,
            quantity: (oldMap.get(l.price) || 0) * 0.7 + l.quantity * 0.3,
          }));
        };

          return {
            bids: blend(prev.bids, bids),
            asks: blend(prev.asks, asks),
          };
        });
        setSnapshotCount((prev) => prev + 1);
        setLastSnapshotAt(now);

        const sortedBids = [...bids].sort((a, b) => b.price - a.price);
        const sortedAsks = [...asks].sort((a, b) => a.price - b.price);

        const bestBid = sortedBids.length ? sortedBids[0].price : null;
        const bestAsk = sortedAsks.length ? sortedAsks[0].price : null;

        if (bestBid != null && bestAsk != null) {
          const rawMid = (bestBid + bestAsk) / 2;

          setMidHistory((prev) => {
            const lastVals = prev.slice(-9).map(p => p.mid);
            const nextVals = [...lastVals, rawMid];

            const smoothed =
              nextVals.reduce((sum, v) => sum + v, 0) / nextVals.length;

            return [
              ...prev,
              { ts: now, time: formatTime(now), mid: smoothed },
            ].slice(-MAX_MID_HISTORY);
          });
        }

        return;
      }

      if (parsed.type === "event") {
        const eventObj = parsed.data;
        setEvents((prev) =>
          [{ ts: now, raw: eventObj }, ...prev].slice(0, MAX_EVENTS)
        );
      }
    };

    return () => {
      ws.close();
    };
  }, []);

  const derived = useMemo(() => {
    const sortedBids = [...orderbook.bids].sort((a, b) => b.price - a.price);
    const sortedAsks = [...orderbook.asks].sort((a, b) => a.price - b.price);

    const bestBid = sortedBids.length ? sortedBids[0].price : null;
    const bestAsk = sortedAsks.length ? sortedAsks[0].price : null;
    const spread =
      bestBid != null && bestAsk != null ? bestAsk - bestBid : null;
    const mid =
    midHistory.length > 0
      ? midHistory[midHistory.length - 1].mid
      : null;

    const totalBidVolume = sortedBids.reduce(
      (sum, level) => sum + (level.quantity || 0),
      0
    );
    const totalAskVolume = sortedAsks.reduce(
      (sum, level) => sum + (level.quantity || 0),
      0
    );
    const totalVolume = totalBidVolume + totalAskVolume;

    const imbalance =
      totalVolume > 0 ? totalBidVolume / totalVolume : null;

    const bestBidSize = sortedBids.length ? sortedBids[0].quantity : null;
    const bestAskSize = sortedAsks.length ? sortedAsks[0].quantity : null;

    const now = Date.now();
    const recentEvents = events.filter((e) => now - e.ts <= ROLLING_WINDOW_MS);

    let trades = 0;
    let tradedVolume = 0;
    let accepts = 0;
    let removals = 0;
    let rejects = 0;
    let cancelFails = 0;

    for (const event of recentEvents) {
      const raw = event.raw;
      if (raw?.TradeExecuted) {
        trades += 1;
        tradedVolume += raw.TradeExecuted.quantity || 0;
      } else if (raw?.OrderAccepted != null) {
        accepts += 1;
      } else if (raw?.OrderRemoved != null) {
        removals += 1;
      } else if (raw?.OrderRejected != null) {
        rejects += 1;
      } else if (raw?.CancellationFailure != null) {
        cancelFails += 1;
      }
    }

    const latestTrade = events.find((e) => e.raw?.TradeExecuted)?.raw?.TradeExecuted;
    const lastTradePrice = latestTrade?.price ?? null;
    const lastTradeQty = latestTrade?.quantity ?? null;

    return {
      bestBid,
      bestAsk,
      spread,
      mid,
      totalBidVolume,
      totalAskVolume,
      imbalance,
      bestBidSize,
      bestAskSize,
      trades,
      tradedVolume,
      accepts,
      removals,
      rejects,
      cancelFails,
      lastTradePrice,
      lastTradeQty,
      levelCount: orderbook.bids.length + orderbook.asks.length,
    };
  }, [orderbook, events]);

  return (
    <div className="app-shell">
      <div className="background-glow glow-1" />
      <div className="background-glow glow-2" />

      <header className="topbar">
        <div>
          <div className="eyebrow">MARKET MONITOR</div>
          <h1 className="hero-title">Orderbook Control Center</h1>
          <p className="hero-subtitle">
            Live market depth, flow, and execution telemetry from your simulation.
          </p>
        </div>

        <div className="connection-pill">
          <span className={`status-dot ${connected ? "live" : "dead"}`} />
          {connected ? "Connected" : "Disconnected"}
        </div>
      </header>

      <section className="metrics-grid">
        <MetricCard
          label="Best Bid"
          value={formatNumber(derived.bestBid)}
          sub={`Size ${formatNumber(derived.bestBidSize)}`}
          accent="green"
        />
        <MetricCard
          label="Best Ask"
          value={formatNumber(derived.bestAsk)}
          sub={`Size ${formatNumber(derived.bestAskSize)}`}
          accent="red"
        />
        <MetricCard
          label="Spread"
          value={formatNumber(derived.spread, 2)}
          sub={derived.mid != null ? `Mid ${formatNumber(derived.mid, 2)}` : "--"}
          accent="violet"
        />
        <MetricCard
          label="Imbalance"
          value={
            derived.imbalance != null
              ? `${formatNumber(derived.imbalance * 100, 1)}%`
              : "--"
          }
          sub={`Bid ${formatNumber(derived.totalBidVolume)} / Ask ${formatNumber(
            derived.totalAskVolume
          )}`}
          accent="cyan"
        />
        <MetricCard
          label="Trades / 10s"
          value={formatNumber(derived.trades)}
          sub={`Vol ${formatNumber(derived.tradedVolume)}`}
          accent="amber"
        />
        <MetricCard
          label="Last Trade"
          value={formatNumber(derived.lastTradePrice)}
          sub={
            derived.lastTradeQty != null
              ? `Qty ${formatNumber(derived.lastTradeQty)}`
              : "No recent trades"
          }
          accent="pink"
        />
      </section>

      <section className="content-grid">
        <div className="left-column">
          <Panel
            title="Depth Curve"
            subtitle="Cumulative liquidity across the visible book"
            rightContent={
              <div className="panel-badge">
                {formatNumber(derived.levelCount)} levels
              </div>
            }
          >
            <DepthChart bids={orderbook.bids} asks={orderbook.asks} />
          </Panel>

          <Panel
            title="Mid Price"
            subtitle="Rolling midpoint from incoming snapshots"
            rightContent={
              <div className="panel-badge">
                {snapshotCount} snapshots
              </div>
            }
          >
            <MidPriceChart data={midHistory} />
          </Panel>
        </div>

        <div className="right-column">
          <Panel
            title="Book Stats"
            subtitle="Live frontend-derived market state"
          >
            <div className="stats-list">
              <StatRow label="Total Bid Depth" value={formatNumber(derived.totalBidVolume)} />
              <StatRow label="Total Ask Depth" value={formatNumber(derived.totalAskVolume)} />
              <StatRow label="Accepted / 10s" value={formatNumber(derived.accepts)} />
              <StatRow label="Removed / 10s" value={formatNumber(derived.removals)} />
              <StatRow label="Rejected / 10s" value={formatNumber(derived.rejects)} />
              <StatRow label="Cancel Fail / 10s" value={formatNumber(derived.cancelFails)} />
              <StatRow
                label="Last Snapshot"
                value={lastSnapshotAt ? formatTime(lastSnapshotAt) : "--"}
              />
            </div>
          </Panel>

          <Panel
            title="Event Stream"
            subtitle="Raw exchange activity in real time"
            rightContent={<div className="panel-badge">{events.length} buffered</div>}
          >
            <div className="event-stream">
              {events.length === 0 ? (
                <div className="empty-state">Waiting for events...</div>
              ) : (
                events.map((event, i) => {
                  const meta = getEventMeta(event.raw);
                  return (
                    <div key={`${event.ts}-${i}`} className={`event-row ${meta.tone}`}>
                      <div className="event-row-top">
                        <span className="event-label">{meta.label}</span>
                        <span className="event-time">{formatTime(event.ts)}</span>
                      </div>
                      <div className="event-summary">{meta.summary}</div>
                      <pre className="event-json">
                        {JSON.stringify(event.raw, null, 2)}
                      </pre>
                    </div>
                  );
                })
              )}
            </div>
          </Panel>
        </div>
      </section>
    </div>
  );
}

function MetricCard({ label, value, sub, accent }) {
  return (
    <div className={`metric-card accent-${accent}`}>
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
      <div className="metric-sub">{sub}</div>
    </div>
  );
}

function Panel({ title, subtitle, rightContent, children }) {
  return (
    <div className="panel">
      <div className="panel-header">
        <div>
          <h2 className="panel-title">{title}</h2>
          <div className="panel-subtitle">{subtitle}</div>
        </div>
        {rightContent}
      </div>
      <div className="panel-body">{children}</div>
    </div>
  );
}

function StatRow({ label, value }) {
  return (
    <div className="stat-row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}