#!/usr/bin/env python3
"""Verify whether a candidate mnemonic or raw seed is the AUGECOIN mainnet
admin key (genesis admin public key 0626021b...).

Usage:
  python3 verify_admin_key.py --mnemonic "word1 word2 ..."
  python3 verify_admin_key.py --hex <64-hex-or-32-byte-hex>

Prints MATCH or NO MATCH. Does NOT touch the chain.
"""
import hashlib
import hmac
import sys

TARGET_PUBKEY = "0626021b8200817447cce14062f69d2f0aaaf0e8838155cbe8ffdfab30002819"


def hkdf_extract(salt, ikm):
    return hmac.new(salt, ikm, hashlib.sha3_512).digest()


def hkdf_expand(prk, info, n):
    okm = b""
    t = b""
    b = 1
    while len(okm) < n:
        t = hmac.new(prk, t + info + bytes([b]), hashlib.sha3_512).digest()
        okm += t
        b += 1
    return okm[:n]


def derive_ed25519_pubkey_from_seed(seed32: bytes) -> str:
    prk = hkdf_extract(b"", seed32)
    info = b"augecoin-hdkey-" + (0).to_bytes(8, "big")
    der = hkdf_expand(prk, info, 64)
    from nacl.signing import SigningKey

    return SigningKey(der[:32]).verify_key.encode().hex()


def from_mnemonic(mnemonic: str) -> str:
    seed = hashlib.pbkdf2_hmac(
        "sha512", mnemonic.encode("utf-8"), b"mnemonic", 2048, 64
    )
    return derive_ed25519_pubkey_from_seed(seed[:32])


def from_hex(hexstr: str) -> str:
    raw = bytes.fromhex(hexstr.strip())
    if len(raw) < 32:
        raise ValueError("hex too short (need >= 32 bytes)")
    return derive_ed25519_pubkey_from_seed(raw[:32])


def main():
    args = sys.argv[1:]
    if "--mnemonic" in args:
        phrase = args[args.index("--mnemonic") + 1]
        pub = from_mnemonic(phrase)
    elif "--hex" in args:
        hexstr = args[args.index("--hex") + 1]
        pub = from_hex(hexstr)
    else:
        print("use --mnemonic \"...\" or --hex <hex>")
        sys.exit(2)
    print("derived admin pubkey:", pub)
    print("MATCH" if pub == TARGET_PUBKEY else "NO MATCH")


if __name__ == "__main__":
    main()
