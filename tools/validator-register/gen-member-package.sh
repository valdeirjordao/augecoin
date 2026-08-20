#!/bin/bash
# AUGECOIN - Generate a per-member Windows validator package.
#
# Usage:
#   ./gen-member-package.sh <member_name> [validator_id]
#
#   member_name  : short id used in file names (no spaces)
#   validator_id : the ValidatorSet id to assign (optional; auto next if omitted)
#
# Produces:
#   out/<member_name>/config.env        (seed + id, secret - do NOT share)
#   out/<member_name>/PUBKEY.txt        (public key to register with validatoradd)
#   out/<member_name>/AUGECOIN-<member_name>-Windows.zip  (installer package)
#
set -e

MEMBER="${1:?uso: $0 <member_name> [validator_id]}"
GEN="/opt/augecoin/target/release/examples/gen_validator_key"
PKG_SRC="/tmp/augecoin-windows"
OUT="out/$MEMBER"
BOOTNODE="/ip4/184.174.36.240/tcp/9200"

mkdir -p "$OUT/bin" "$OUT/scripts"

# --- 1. generate unique keypair for this member ---
KEYGEN=$("$GEN")
SEED=$(echo "$KEYGEN" | grep '^AUGECOIN_VALIDATOR_KEY_HEX=' | cut -d= -f2)
PUBKEY=$(echo "$KEYGEN" | grep '^ED25519_PUBLIC_KEY_HEX=' | cut -d= -f2)

# --- 2. resolve validator id ---
VID="${2:-}"
if [ -z "$VID" ]; then
  # auto: read the last assigned id from out/.last_id, else 5
  VID=$(cat out/.last_id 2>/dev/null || echo 5)
fi
NEXT=$((VID + 1))
echo "$NEXT" > out/.last_id

# --- 3. write config.env (secret) ---
cat > "$OUT/config.env" <<EOF
AUGECOIN_VALIDATOR_KEY_HEX=$SEED
AUGECOIN_VALIDATOR_ID=$VID
AUGECOIN_VALIDATOR_COUNT=4
AUGECOIN_CHAIN_ID=2
AUGECOIN_BOOTNODES=$BOOTNODE
AUGECOIN_DATA_DIR=/opt/augecoin/data
AUGECOIN_RPC_PORT=9005
AUGECOIN_WALLET_PORT=8081
AUGECOIN_METRICS_PORT=9100
AUGECOIN_P2P_PORT=9200
AUGECOIN_BLOCK_TIME=15
AUGECOIN_MAX_TRANSMIT_SIZE=2097152
EOF
chmod 600 "$OUT/config.env"

# --- 4. write PUBKEY (to register in the network) ---
echo "$PUBKEY" > "$OUT/PUBKEY.txt"
echo "Member : $MEMBER"
echo "ID     : $VID"
echo "PUBKEY : $PUBKEY"
echo "-------------------------------------------"
echo "REGISTRE esta chave publica na rede:"
echo "  validatoradd --ed25519_key $PUBKEY --activation_height <altura>"
echo "-------------------------------------------"

# --- 5. assemble the package ---
cp "$PKG_SRC/bin/augecoin-node"      "$OUT/bin/"
cp "$PKG_SRC/bin/gen_validator_key"  "$OUT/bin/"
cp "$PKG_SRC/scripts/bootstrap.sh"   "$OUT/scripts/"
cp "$PKG_SRC/instalar-validor.bat"   "$OUT/"
cp "$PKG_SRC/install-validator.ps1"  "$OUT/"

cd "$OUT"
rm -f "AUGECOIN-$MEMBER-Windows.zip"
zip -rq "AUGECOIN-$MEMBER-Windows.zip" . -x "*.txt" -x "config.env" -x "PUBKEY.txt"
cd - >/dev/null

echo ""
echo "Pacote gerado: $OUT/AUGECOIN-$MEMBER-Windows.zip"
echo "  - Envie o .zip para o membro (contem seed unico embutido)."
echo "  - Guarde config.env (backup) e NAO compartilhe."
