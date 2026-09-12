"""
payment-gateway-example/server.py

Backend de gateway de pagamento AUGE usando Python SDK.

Fluxo:
  1. POST /api/payment/create  → cria sessão de pagamento
  2. GET  /api/payment/status/:id → consulta status
  3. POST /api/webhook         → recebe notificação de confirmação
  4. GET  /api/orders          → lista pedidos

Uso:
  pip install fastapi uvicorn
  uvicorn server:app --host 0.0.0.0 --port 8000
"""

import asyncio
import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from typing import Optional

import httpx
from fastapi import FastAPI, Request, HTTPException
from fastapi.responses import HTMLResponse, JSONResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

# ─── Config ───────────────────────────────────────────────────────────────────

RPC_URL = os.getenv("AUGECOIN_RPC_URL", "http://localhost:4003")
MERCHANT_ACCOUNT = int(os.getenv("MERCHANT_ACCOUNT", "0"))
PORT = int(os.getenv("PORT", "8000"))

# ─── Import SDK ───────────────────────────────────────────────────────────────

import sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "packages", "augecoin-sdk-python", "src"))

from augecoin_sdk import (
    AugecoinClient, AugecoinWalletClient, Chain,
    KeyPair, HdWallet, build_transfer, op_hex,
    auge_to_augesat, augesat_to_auge, format_auge, AUGESAT_PER_AUGE,
)

# ─── In-memory order DB ───────────────────────────────────────────────────────

ORDERS: dict[str, dict] = {}
PAYMENT_SESSIONS: dict[str, dict] = {}

# ─── RPC client ───────────────────────────────────────────────────────────────

client = AugecoinClient(AugecoinClientConfig(rpc_url=RPC_URL))

# ─── FastAPI app ──────────────────────────────────────────────────────────────

app = FastAPI(title="AugeCoin Payment Gateway", version="1.0.0")


class CreatePaymentRequest(BaseModel):
    order_id: str
    amount_auge: float
    memo: Optional[str] = None


class PaymentResponse(BaseModel):
    session_id: str
    order_id: str
    amount_auge: float
    amount_augesat: int
    status: str
    merchant_account: int
    payment_uri: str
    qr_data: str


@app.get("/health")
async def health():
    return {"status": "ok", "rpc": RPC_URL}


@app.post("/api/payment/create", response_model=PaymentResponse)
async def create_payment(req: CreatePaymentRequest):
    """Create a payment session for a given order."""
    if req.amount_auge <= 0:
        raise HTTPException(400, "amount_auge must be positive")

    session_id = str(uuid.uuid4())
    amount_augesat = auge_to_augesat(req.amount_auge)

    # Store order
    order = {
        "id": req.order_id,
        "amount_auge": req.amount_auge,
        "amount_augesat": amount_augesat,
        "status": "pending",
        "session_id": session_id,
        "created_at": datetime.now(timezone.utc).isoformat(),
    }
    ORDERS[req.order_id] = order
    PAYMENT_SESSIONS[session_id] = order

    # Build payment URI
    payment_uri = (
        f"augecoin:{MERCHANT_ACCOUNT}"
        f"?amount={req.amount_auge:.8f}"
        f"&label=AugeCoin+Payment"
        f"&message={req.memo or req.order_id}"
    )

    return PaymentResponse(
        session_id=session_id,
        order_id=req.order_id,
        amount_auge=req.amount_auge,
        amount_augesat=amount_augesat,
        status="pending",
        merchant_account=MERCHANT_ACCOUNT,
        payment_uri=payment_uri,
        qr_data=payment_uri,
    )


@app.get("/api/payment/status/{session_id}")
async def payment_status(session_id: str):
    """Check if a payment session has been confirmed."""
    order = PAYMENT_SESSIONS.get(session_id)
    if not order:
        raise HTTPException(404, "Session not found")

    return {
        "session_id": session_id,
        "order_id": order["id"],
        "status": order["status"],
        "amount_augesat": order["amount_augesat"],
        "amount_auge": order["amount_auge"],
    }


@app.get("/api/payment/verify")
async def verify_payment(transaction_hash: str, expected_amount_augesat: int):
    """
    Verify an on-chain transaction directly.
    Developer calls this when a payer provides a tx hash.
    """
    try:
        result = await client.getOperationByHash(transaction_hash)
        if not result:
            return {"valid": False, "error": "Transaction not found"}

        actual_amount = int(result.get("amount", 0))
        valid = actual_amount == expected_amount_augesat
        return {
            "valid": valid,
            "transaction_hash": transaction_hash,
            "amount_augesat": actual_amount,
            "block_number": result.get("block_number"),
            "sender_account": result.get("sender_account"),
            "receiver_account": result.get("receiver_account"),
        }
    except Exception as e:
        return {"valid": False, "error": str(e)}


@app.post("/api/webhook")
async def webhook(request: Request):
    """
    Webhook called by the payment gateway SDK when a payment is confirmed.
    Verify signature with x-augecoin-signature header.
    """
    body = await request.body()
    signature = request.headers.get("x-augecoin-signature", "")
    event_type = request.headers.get("x-augecoin-event", "")

    payload = httpx._content.encode_json(body)[0] if hasattr(httpx, '_content') else {}
    try:
        import json
        payload = json.loads(body)
    except Exception:
        pass

    print(f"[webhook] event={event_type} payload={payload}")

    # Mark order as paid
    session_id = payload.get("sessionId")
    if session_id:
        order = PAYMENT_SESSIONS.get(session_id)
        if order:
            order["status"] = "paid"
            order["transaction_hash"] = payload.get("transactionHash")
            order["confirmed_at"] = datetime.now(timezone.utc).isoformat()
            print(f"[order] {order['id']} PAID — tx={payload.get('transactionHash')}")
            # TODO: fulfill order here (send confirmation email, grant access, etc.)

    return JSONResponse({"received": True})


@app.get("/api/orders")
async def list_orders():
    """List all orders (for demo)."""
    return list(ORDERS.values())


@app.get("/")
async def index():
    return HTMLResponse("""
    <h1>AugeCoin Payment Gateway</h1>
    <p>POST to /api/payment/create to start a payment</p>
    <pre>curl -X POST http://localhost:8000/api/payment/create \
  -H "Content-Type: application/json" \
  -d '{"order_id": "order-001", "amount_auge": 0.05}'</pre>
    """)


if __name__ == "__main__":
    import uvicorn
    print(f"""
╔══════════════════════════════════════════════════════════════╗
║         AugeCoin Payment Gateway (Python example)            ║
╠══════════════════════════════════════════════════════════════╣
║  API:    http://localhost:{PORT}                              ║
║  RPC:    {RPC_URL:<48}║
║  Merchant acct: {str(MERCHANT_ACCOUNT):<40}║
╚══════════════════════════════════════════════════════════════╝
""")
    uvicorn.run(app, host="0.0.0.0", port=PORT)
