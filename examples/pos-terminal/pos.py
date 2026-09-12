"""
pos-terminal-example/pos.py

Terminal Ponto de Venda (POS) com pagamento AUGE.

Fluxo:
  1. Atendente informa o valor da venda
  2. Sistema gera um QR code com o pagamento URI
  3. Cliente escaneia com sua carteira e paga
  4. Sistema confirma a inclusão em bloco
  5. Venda é finalizada

Requisitos:
  pip install fastapi uvicorn qrcode[pil]
"""

import asyncio
import os
import uuid
from datetime import datetime, timezone
from typing import Optional

import qrcode
import uvicorn
from fastapi import FastAPI, Request
from fastapi.responses import HTMLResponse, JSONResponse
from pydantic import BaseModel

import sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "packages", "augecoin-sdk-python", "src"))

from augecoin_sdk import (
    AugecoinClient, Chain,
    auge_to_augesat, augesat_to_auge, format_auge,
)

RPC_URL = os.getenv("AUGECOIN_RPC_URL", "http://localhost:4003")
MERCHANT_ACCOUNT = int(os.getenv("MERCHANT_ACCOUNT", "0"))
PORT = int(os.getenv("PORT", "8001"))

client = AugecoinClient(AugecoinClientConfig(rpc_url=RPC_URL))
app = FastAPI(title="AugeCoin POS Terminal")

# In-memory transaction DB
SALES: dict[str, dict] = {}


class CreateSaleRequest(BaseModel):
    amount_auge: float
    description: str
    cashier: Optional[str] = None


@app.get("/health")
async def health():
    try:
        status = await client.get_node_status()
        return {"status": "ok", "block_height": status.get("block_height")}
    except Exception as e:
        return {"status": "error", "detail": str(e)}


@app.post("/api/pos/sale")
async def create_sale(req: CreateSaleRequest):
    """Start a new POS sale."""
    sale_id = str(uuid.uuid4())
    amount_augesat = auge_to_augesat(req.amount_auge)

    # Generate QR code
    payment_uri = (
        f"augecoin:{MERCHANT_ACCOUNT}"
        f"?amount={req.amount_auge:.8f}"
        f"&label={req.description.replace(' ', '+')}"
    )
    qr = qrcode.make(payment_uri)
    qr_path = f"/tmp/qr_{sale_id[:8]}.png"
    qr.save(qr_path)

    sale = {
        "id": sale_id,
        "amount_auge": req.amount_auge,
        "amount_augesat": amount_augesat,
        "description": req.description,
        "cashier": req.cashier,
        "status": "awaiting_payment",
        "payment_uri": payment_uri,
        "qr_path": qr_path,
        "created_at": datetime.now(timezone.utc).isoformat(),
        "confirmed_at": None,
        "transaction_hash": None,
    }
    SALES[sale_id] = sale

    return JSONResponse({
        "sale_id": sale_id,
        "amount_auge": req.amount_auge,
        "amount_augesat": amount_augesat,
        "description": req.description,
        "payment_uri": payment_uri,
        "qr_code_url": f"/api/pos/qr/{sale_id[:8]}",
        "status": "awaiting_payment",
        "merchant_account": MERCHANT_ACCOUNT,
        "expires_in_seconds": 300,
    })


@app.get("/api/pos/qr/{sale_short_id}")
async def get_qr(sale_short_id: str):
    """Return the QR code PNG for a sale."""
    qr_path = f"/tmp/qr_{sale_short_id}.png"
    try:
        from fastapi.responses import FileResponse
        return FileResponse(qr_path, media_type="image/png")
    except FileNotFoundError:
        raise HTTPException(404, "QR code not found")


@app.get("/api/pos/sale/{sale_id}")
async def get_sale_status(sale_id: str):
    """Check sale status."""
    sale = SALES.get(sale_id)
    if not sale:
        raise HTTPException(404, "Sale not found")
    return sale


@app.post("/api/pos/sale/{sale_id}/cancel")
async def cancel_sale(sale_id: str):
    """Cancel an awaiting sale."""
    sale = SALES.get(sale_id)
    if not sale:
        raise HTTPException(404, "Sale not found")
    if sale["status"] != "awaiting_payment":
        raise HTTPException(400, "Sale already processed")
    sale["status"] = "cancelled"
    sale["cancelled_at"] = datetime.now(timezone.utc).isoformat()
    return {"sale_id": sale_id, "status": "cancelled"}


@app.get("/api/pos/sales")
async def list_sales(status: Optional[str] = None):
    """List all sales, optionally filtered by status."""
    sales = list(SALES.values())
    if status:
        sales = [s for s in sales if s["status"] == status]
    return sales


@app.get("/api/pos/balance")
async def get_balance():
    """Get merchant's current AUGE balance."""
    try:
        account = await client.get_account(MERCHANT_ACCOUNT)
        balance_augesat = int(account.get("balance", 0))
        return {
            "merchant_account": MERCHANT_ACCOUNT,
            "balance_augesat": balance_augesat,
            "balance_auge": format_auge(balance_augesat),
        }
    except Exception as e:
        return {"error": str(e)}


@app.get("/", response_class=HTMLResponse)
async def index():
    return """
    <html>
    <head><title>AugeCoin POS</title>
    <style>
      body { font-family: monospace; background: #0a0e17; color: #e8eaed; padding: 40px; }
      h1 { color: #ffd700; }
      a { color: #6ea8fe; }
      code { background: #141925; padding: 2px 6px; border-radius: 4px; }
    </style>
    </head>
    <body>
      <h1>AugeCoin POS Terminal</h1>
      <p>POST <code>/api/pos/sale</code> to start a sale</p>
      <p>GET  <code>/api/pos/sales</code> to list all sales</p>
      <p>GET  <code>/api/pos/balance</code> to check merchant balance</p>
      <pre>
curl -X POST http://localhost:8001/api/pos/sale \\
  -H "Content-Type: application/json" \\
  -d '{"amount_auge": 0.05, "description": "Coffee"}'
      </pre>
    </body>
    </html>
    """


if __name__ == "__main__":
    print(f"""
╔══════════════════════════════════════════════════════════════╗
║              AugeCoin POS Terminal                           ║
╠══════════════════════════════════════════════════════════════╣
║  Server:  http://localhost:{PORT}                              ║
║  RPC:     {RPC_URL:<48}║
║  Merchant: {str(MERCHANT_ACCOUNT):<43}║
╚══════════════════════════════════════════════════════════════╝
""")
    uvicorn.run(app, host="0.0.0.0", port=PORT)
