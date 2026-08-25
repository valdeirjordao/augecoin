/* tslint:disable */
/* eslint-disable */

export class WasmHdWallet {
    free(): void;
    [Symbol.dispose](): void;
    ed25519_public_key_hex(index: bigint): string;
    constructor(mnemonic: string);
    sign(index: bigint, message: string): string;
    /**
     * Sign the *bytes* encoded by `message_hex` (hex string) at the given index.
     */
    sign_hex(index: bigint, message_hex: string): string;
}

export class WasmKeyPair {
    free(): void;
    [Symbol.dispose](): void;
    ed25519_public_key_hex(): string;
    constructor();
    sign(message: string): string;
    /**
     * Sign the *bytes* encoded by `message_hex` (hex string), returning the
     * signature as a hex string. This is the canonical entry point for
     * signing arbitrary binary messages (e.g. serialized operations).
     */
    sign_hex(message_hex: string): string;
    verify(message: string, signature_hex: string): boolean;
    /**
     * Verify a signature over the *bytes* encoded by `message_hex`.
     */
    verify_hex(message_hex: string, signature_hex: string): boolean;
}
