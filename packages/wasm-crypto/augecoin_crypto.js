/* @ts-self-types="./augecoin_crypto.d.ts" */
import * as wasm from "./augecoin_crypto_bg.wasm";
import { __wbg_set_wasm } from "./augecoin_crypto_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    WasmHdWallet, WasmKeyPair
} from "./augecoin_crypto_bg.js";
