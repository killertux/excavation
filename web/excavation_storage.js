// Excavation web storage bridge (localStorage).
//
// The vendored macroquad JS bundle (mq_js_bundle.js) does not ship a storage
// plugin, and the `quad-storage` plugin JS depends on `sapp-jsutils` globals that
// this bundle keeps in an IIFE (so they are not available). Instead, this small
// plugin registers a handful of `exc_save_*` env functions that read/write
// `window.localStorage` directly, converting between Rust UTF-8 byte buffers and
// JS strings through the global `wasm_memory` (a top-level `var` in the bundle).
//
// It must be loaded AFTER mq_js_bundle.js (for `miniquad_add_plugin`) and BEFORE
// `load("excavation.wasm")`. See `index.html` and `build-web.sh`.
(function () {
    "use strict";

    // Decode `len` UTF-8 bytes starting at `ptr` in wasm memory.
    function read_string(ptr, len) {
        return new TextDecoder().decode(new Uint8Array(wasm_memory.buffer, ptr, len));
    }

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.exc_save_set = function (key_ptr, key_len, val_ptr, val_len) {
                var key = read_string(key_ptr, key_len);
                var value = read_string(val_ptr, val_len);
                localStorage.setItem(key, value);
            };
            importObject.env.exc_save_get_len = function (key_ptr, key_len) {
                var key = read_string(key_ptr, key_len);
                var value = localStorage.getItem(key);
                if (value == null) {
                    return -1;
                }
                // Byte length of the UTF-8 encoding (so Rust can size its buffer).
                return new TextEncoder().encode(value).length;
            };
            importObject.env.exc_save_get_into = function (key_ptr, key_len, out_ptr, out_cap) {
                var key = read_string(key_ptr, key_len);
                var value = localStorage.getItem(key);
                if (value == null) {
                    return -1;
                }
                var encoded = new TextEncoder().encode(value);
                var n = Math.min(encoded.length, out_cap);
                new Uint8Array(wasm_memory.buffer, out_ptr, n).set(encoded.subarray(0, n));
                return n;
            };
            importObject.env.exc_save_remove = function (key_ptr, key_len) {
                localStorage.removeItem(read_string(key_ptr, key_len));
            };
        },
        name: "excavation_storage",
        version: 1,
    });
})();
