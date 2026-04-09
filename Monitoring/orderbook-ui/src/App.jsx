// src/App.jsx

import { useEffect, useRef, useState } from "react";
import DepthChart from "./DepthChart";

export default function App() {
  const [connected, setConnected] = useState(false);
  const [events, setEvents] = useState([]);
  const [orderbook, setOrderbook] = useState({ bids: [], asks: [] });
  const wsRef = useRef(null);

  useEffect(() => {
    const ws = new WebSocket("ws://127.0.0.1:9001");
    wsRef.current = ws;

    ws.onopen = () => {
      console.log("Connected");
      setConnected(true);
      ws.send("subscribe");
    };

    ws.onclose = () => {
      console.log("Disconnected");
      setConnected(false);
    };

    ws.onerror = (err) => {
      console.error("WebSocket error:", err);
    };

    ws.onmessage = (msg) => {
      const data = JSON.parse(msg.data);

      if (data.type === "snapshot") {
        setOrderbook({
          bids: data.data.bid_infos || [],
          asks: data.data.ask_infos || [],
        });
        return;
      }

      if (data.type === "event") {
        setEvents((prev) => [data.data, ...prev].slice(0, 100));
      }
    };

    return () => {
      ws.close();
    };
  }, []);

  return (
    <div style={{ padding: "20px", fontFamily: "Arial" }}>
      <h2>Orderbook UI</h2>

      <div>
        Status:{" "}
        <span style={{ color: connected ? "green" : "red" }}>
          {connected ? "Connected" : "Disconnected"}
        </span>
      </div>

      <div style={{ marginTop: "20px" }}>
        <DepthChart bids={orderbook.bids} asks={orderbook.asks} />
      </div>

      <div style={{ marginTop: "30px" }}>
        <h3>Events</h3>
        <div
          style={{
            maxHeight: "220px",
            overflow: "auto",
            background: "#111",
            color: "#0f0",
            padding: "10px",
            fontSize: "12px",
            borderRadius: "8px",
          }}
        >
          {events.map((e, i) => (
            <div key={i}>{JSON.stringify(e)}</div>
          ))}
        </div>
      </div>
    </div>
  );
}