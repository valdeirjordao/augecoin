/* tslint:disable */
/* eslint-disable */
export const memory: WebAssembly.Memory;
export const __wbg_wasmhdwallet_free: (a: number, b: number) => void;
export const __wbg_wasmkeypair_free: (a: number, b: number) => void;
export const wasmhdwallet_ed25519_public_key_hex: (a: number, b: bigint) => [number, number];
export const wasmhdwallet_new: (a: number, b: number) => [number, number, number];
export const wasmhdwallet_sign: (a: number, b: bigint, c: number, d: number) => [number, number];
export const wasmhdwallet_sign_hex: (a: number, b: bigint, c: number, d: number) => [number, number, number, number];
export const wasmkeypair_ed25519_public_key_hex: (a: number) => [number, number];
export const wasmkeypair_new: () => number;
export const wasmkeypair_sign: (a: number, b: number, c: number) => [number, number];
export const wasmkeypair_sign_hex: (a: number, b: number, c: number) => [number, number, number, number];
export const wasmkeypair_verify: (a: number, b: number, c: number, d: number, e: number) => number;
export const wasmkeypair_verify_hex: (a: number, b: number, c: number, d: number, e: number) => number;
export const __wbindgen_exn_store: (a: number) => void;
export const __externref_table_alloc: () => number;
export const __wbindgen_externrefs: WebAssembly.Table;
export const __wbindgen_free: (a: number, b: number, c: number) => void;
export const __wbindgen_malloc: (a: number, b: number) => number;
export const __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
export const __externref_table_dealloc: (a: number) => void;
export const __wbindgen_start: () => void;
