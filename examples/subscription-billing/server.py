"""
subscription-billing-example/server.py

Backend de assinatura recorrente com pagamento em AUGE.

Conceitos:
  - Planos de assinatura (monthly, yearly, etc.)
  - Cobrança automática em intervalo
  - Grace period para pagamentos atrasados
  - Retry com backoff exponencial
  - Webhooks para eventos de billing

Uso:
  pip install fastapi uvicorn
  uvicorn server:app --host 0.0.0.0 --port 8002
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Optional

import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

import sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "packages", "augecoin-sdk-python", "src"))

from augecoin_sdk import (
    AugecoinClient, AugecoinWalletClient, Chain, HdWallet,
    auge_to_augesat, augesat_to_auge, format_auge, AUGESAT_PER_AUGE,
)

RPC_URL = os.getenv("AUGECOIN_RPC_URL", "http://localhost:4003")
PORT = int(os.getenv("PORT", "8002"))

client = AugecoinClient(AugecoinClientConfig(rpc_url=RPC_URL))
app = FastAPI(title="AugeCoin Subscription Billing", version="1.0.0")

# ─── In-memory stores ─────────────────────────────────────────────────────────

PLANS: dict[str, dict] = {}
SUBSCRIPTIONS: dict[str, dict] = {}


# ─── Plan management ──────────────────────────────────────────────────────────

class CreatePlanRequest(BaseModel):
    name: str
    description: Optional[str] = None
    amount_auge: float
    interval: str = "month"  # hour | day | week | month | year
    interval_count: int = 1
    trial_days: Optional[int] = None


@app.post("/api/subscriptions/plans")
async def create_plan(req: CreatePlanRequest):
    plan_id = str(uuid.uuid4())
    plan = {
        "id": plan_id,
        "name": req.name,
        "description": req.description,
        "amount_auge": req.amount_auge,
        "amount_augesat": auge_to_augesat(req.amount_auge),
        "interval": req.interval,
        "interval_count": req.interval_count,
        "trial_days": req.trial_days,
        "active": True,
        "created_at": datetime.now(timezone.utc).isoformat(),
    }
    PLANS[plan_id] = plan
    return plan


@app.get("/api/subscriptions/plans")
async def list_plans():
    return list(PLANS.values())


@app.get("/api/subscriptions/plans/{plan_id}")
async def get_plan(plan_id: str):
    plan = PLANS.get(plan_id)
    if not plan:
        raise HTTPException(404, "Plan not found")
    return plan


# ─── Subscription management ──────────────────────────────────────────────────

class CreateSubscriptionRequest(BaseModel):
    plan_id: str
    customer_account: int
    trial_days: Optional[int] = None
    metadata: Optional[dict] = None


@app.post("/api/subscriptions")
async def create_subscription(req: CreateSubscriptionRequest):
    plan = PLANS.get(req.plan_id)
    if not plan:
        raise HTTPException(404, "Plan not found")

    sub_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)
    trial_end = now  # default: no trial

    if req.trial_days or plan.get("trial_days"):
        from datetime import timedelta
        trial_days = req.trial_days or plan["trial_days"]
        trial_end = now + timedelta(days=trial_days)

    sub = {
        "id": sub_id,
        "plan_id": req.plan_id,
        "plan_name": plan["name"],
        "amount_auge": plan["amount_auge"],
        "amount_augesat": plan["amount_augesat"],
        "customer_account": req.customer_account,
        "status": "trialing" if trial_end > now else "active",
        "trial_end": trial_end.isoformat(),
        "current_period_start": now.isoformat(),
        "current_period_end": _next_billing_date(now, plan["interval"], plan["interval_count"]).isoformat(),
        "failed_attempts": 0,
        "metadata": req.metadata or {},
        "created_at": now.isoformat(),
    }
    SUBSCRIPTIONS[sub_id] = sub
    return sub


@app.get("/api/subscriptions")
async def list_subscriptions(customer_account: Optional[int] = None):
    subs = list(SUBSCRIPTIONS.values())
    if customer_account is not None:
        subs = [s for s in subs if s["customer_account"] == customer_account]
    return subs


@app.get("/api/subscriptions/{sub_id}")
async def get_subscription(sub_id: str):
    sub = SUBSCRIPTIONS.get(sub_id)
    if not sub:
        raise HTTPException(404, "Subscription not found")
    return sub


@app.post("/api/subscriptions/{sub_id}/bill-now")
async def bill_now(sub_id: str):
    """Manually trigger an immediate billing cycle."""
    sub = SUBSCRIPTIONS.get(sub_id)
    if not sub:
        raise HTTPException(404, "Subscription not found")

    # In production: build and sign a transaction, send to chain
    # Here we just record the billing attempt
    sub["last_billed_at"] = datetime.now(timezone.utc).isoformat()
    sub["status"] = "active"
    return {
        "subscription_id": sub_id,
        "status": "charged",
        "amount_augesat": sub["amount_augesat"],
        "amount_auge": sub["amount_auge"],
        "billed_at": sub["last_billed_at"],
    }


@app.post("/api/subscriptions/{sub_id}/cancel")
async def cancel_subscription(sub_id: str, immediate: bool = False):
    """Cancel a subscription."""
    sub = SUBSCRIPTIONS.get(sub_id)
    if not sub:
        raise HTTPException(404, "Subscription not found")
    sub["status"] = "cancelled"
    sub["cancelled_at"] = datetime.now(timezone.utc).isoformat()
    return {"subscription_id": sub_id, "status": "cancelled"}


@app.post("/api/subscriptions/{sub_id}/webhook")
async def subscription_webhook(sub_id: str, request: Request):
    """Receive webhook events for a subscription."""
    body = await request.body()
    print(f"[subscription webhook] sub={sub_id} body={body.decode()}")
    return {"received": True}


# ─── Helpers ──────────────────────────────────────────────────────────────────

INTERVAL_MS = {
    "hour": 3600 * 1000,
    "day": 86400 * 1000,
    "week": 7 * 86400 * 1000,
    "month": 30 * 86400 * 1000,
    "year": 365 * 86400 * 1000,
}


def _next_billing_date(from_date, interval, count):
    from datetime import timedelta
    ms = INTERVAL_MS.get(interval, 30 * 86400 * 1000) * count
    return from_date + timedelta(milliseconds=ms)


# ─── Entry point ──────────────────────────────────────────────────────────────

@app.get("/")
async def index():
    return """
    <html><body style="font-family:monospace;background:#0a0e17;color:#e8eaed;padding:40px">
      <h1 style="color:#ffd700">AugeCoin Subscription Billing</h1>
      <p>POST /api/subscriptions/plans — create a plan</p>
      <p>POST /api/subscriptions — subscribe to a plan</p>
      <p>POST /api/subscriptions/{id}/bill-now — trigger billing</p>
      <pre>curl -X POST http://localhost:8002/api/subscriptions/plans \
  -H "Content-Type: application/json" \
  -d '{"name":"Premium","amount_auge":0.5,"interval":"month"}'</pre>
    </body></html>
    """


if __name__ == "__main__":
    print(f"""
╔══════════════════════════════════════════════════════════════╗
║       AugeCoin Subscription Billing                         ║
╠══════════════════════════════════════════════════════════════╣
║  Server:  http://localhost:{PORT}                              ║
║  RPC:     {RPC_URL:<48}║
╚══════════════════════════════════════════════════════════════╝
""")
    uvicorn.run(app, host="0.0.0.0", port=PORT)
