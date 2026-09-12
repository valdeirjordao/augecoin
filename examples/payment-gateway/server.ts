/**
 * payment-gateway-example/server.ts
 *
 * Payment Gateway backend — Express.js
 *
 * Flow:
 *  1. Client requests a payment session  → POST /api/payment/create
 *  2. Server returns payment URI + QR data
 *  3. Client monitors for confirmation    → GET  /api/payment/status/:id
 *  4. Webhook fires on confirmation       → POST /api/webhook
 *  5. Merchant fulfills the order
 *
 * Run:
 *  npm install && npx tsx server.ts
 *
 * Then open http://localhost:3000
 */

import express from "express";
import cors from "cors";
import { PaymentGateway, RpcClient, HdWallet, AUGESAT_PER_AUGE, DECIMALS } from "@augecoin/sdk";

// ─── Configuration ───────────────────────────────────────────────────────────

const RPC_URL = process.env.AUGECOIN_RPC_URL ?? "http://localhost:4003";
const MERCHANT_ACCOUNT = Number(process.env.MERCHANT_ACCOUNT ?? "0");
const PORT = Number(process.env.PORT ?? "3000");
const ORDER_DB = new Map<string, Order>();

interface Order {
  id: string;
  amountAuge: number;
  status: "pending" | "paid" | "fulfilled" | "cancelled";
  paymentSessionId?: string;
  createdAt: number;
}

// ─── Init SDK ────────────────────────────────────────────────────────────────

const rpc = new RpcClient({ rpcUrl: RPC_URL });
const gateway = new PaymentGateway({
  rpcUrl: RPC_URL,
  merchantAccount: MERCHANT_ACCOUNT,
  confirmations: 1,
});

// Listen for payment events
gateway.on(async (event) => {
  console.log(`[gateway] event=${event.type} session=${event.session.id} amount=${event.session.amountAuge} AUGE`);
  if (event.type === "payment.confirmed") {
    const order = ORDER_DB.get(event.session.id);
    if (order) {
      order.status = "paid";
      console.log(`[order] Order ${order.id} PAID — fulfilling…`);
      // TODO: fulfill the order (send email, grant access, ship item, etc.)
      order.status = "fulfilled";
    }
  }
});

// ─── Express app ─────────────────────────────────────────────────────────────

const app = express();
app.use(cors());
app.use(express.json());

// Health
app.get("/health", (_req, res) => res.json({ status: "ok", chain: "devnet", rpc: RPC_URL }));

// ─── Create payment session ───────────────────────────────────────────────────

app.post("/api/payment/create", async (req, res) => {
  try {
    const { orderId, amountAuge, memo, currency } = req.body as {
      orderId: string;
      amountAuge: number;
      memo?: string;
      currency?: string;
    };

    if (!orderId || !amountAuge || amountAuge <= 0) {
      return res.status(400).json({ error: "orderId and positive amountAuge are required" });
    }

    const session = await gateway.createSession({
      amountAuge,
      memo: memo ?? `Order ${orderId}`,
      metadata: { orderId, currency: currency ?? "AUGE" },
    });

    const order: Order = {
      id: orderId,
      amountAuge,
      status: "pending",
      paymentSessionId: session.id,
      createdAt: Date.now(),
    };
    ORDER_DB.set(session.id, order);

    // Generate payment URI for QR code
    const address = await resolveMerchantAddress();
    const paymentUri = gateway.paymentUri(address, {
      amountAuge,
      label: "AugeCoin Payment",
      memo: `Order ${orderId}`,
    });

    res.json({
      sessionId: session.id,
      orderId,
      amountAuge,
      status: session.status,
      expiresAt: new Date(session.expiresAt).toISOString(),
      paymentUri,
      qrData: paymentUri,
      merchantAccount: MERCHANT_ACCOUNT,
      confirmationsRequired: session.requiredConfirmations,
    });
  } catch (e: any) {
    console.error("/api/payment/create error:", e);
    res.status(500).json({ error: e.message });
  }
});

// ─── Check payment status ─────────────────────────────────────────────────────

app.get("/api/payment/status/:sessionId", async (req, res) => {
  try {
    const { sessionId } = req.params;
    const result = await gateway.verifyPayment({ sessionId });

    res.json({
      sessionId,
      confirmed: result.confirmed,
      status: result.session?.status ?? "unknown",
      error: result.error,
      transactionHash: result.session?.transactionHash,
      blockNumber: result.session?.blockNumber,
    });
  } catch (e: any) {
    res.status(500).json({ error: e.message });
  }
});

// ─── Webhook endpoint (called by gateway on payment events) ──────────────────

app.post("/api/webhook", express.raw({ type: "*/*" }), (req, res) => {
  const signature = req.headers["x-augecoin-signature"] as string;
  const eventType = req.headers["x-augecoin-event"] as string;
  const body = req.body.toString();

  // Verify webhook signature (in production use HMAC verification)
  console.log(`[webhook] event=${eventType} signature=${signature}`);
  console.log(`[webhook] body=${body}`);

  const payload = JSON.parse(body);
  console.log(`[webhook] session=${payload.sessionId} amount=${payload.amountAugesat} augesat`);

  res.status(200).json({ received: true });
});

// ─── List orders (demo) ──────────────────────────────────────────────────────

app.get("/api/orders", (_req, res) => {
  const orders = Array.from(ORDER_DB.values()).map((o) => ({
    id: o.id,
    amountAuge: o.amountAuge,
    status: o.status,
    createdAt: new Date(o.createdAt).toISOString(),
  }));
  res.json(orders);
});

// ─── Helper: resolve merchant address ───────────────────────────────────────

async function resolveMerchantAddress(): Promise<string> {
  try {
    if (typeof MERCHANT_ACCOUNT === "number" && MERCHANT_ACCOUNT > 0) {
      const acct = await rpc.getAccount(MERCHANT_ACCOUNT);
      return acct.account_key?.public_key_hex ?? "auge1demo";
    }
  } catch { /* fallthrough */ }
  return "auge1demo";
}

// ─── Start ───────────────────────────────────────────────────────────────────

app.listen(PORT, () => {
  console.log(`
╔══════════════════════════════════════════════════════════════╗
║              AugeCoin Payment Gateway (example)              ║
╠══════════════════════════════════════════════════════════════╣
║  Server:  http://localhost:${PORT}                            ║
║  RPC:     ${RPC_URL.padEnd(48)}║
║  Merchant account: ${String(MERCHANT_ACCOUNT).padEnd(42)}║
║                                                              ║
║  POST /api/payment/create  — create a payment session        ║
║  GET  /api/payment/status/:id — check payment status         ║
║  POST /api/webhook          — receive gateway webhooks       ║
║  GET  /api/orders           — list all orders                ║
╚══════════════════════════════════════════════════════════════╝
  `);
});
