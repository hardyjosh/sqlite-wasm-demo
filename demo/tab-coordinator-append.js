// Embedded-wasm shim (works with wasm_bindgen("./tab_coordinator_shared_worker_bg.wasm"))
(function(){
  self.__WASM_B64_MAP = self.__WASM_B64_MAP || {};
  self.__b64ToU8 = self.__b64ToU8 || (function(){
    const dec = typeof atob === 'function' ? atob : (b)=>Buffer.from(b,'base64').toString('binary');
    return function(b64){ const s=dec(b64); const u8=new Uint8Array(s.length); for (let i=0;i<s.length;i++) u8[i]=s.charCodeAt(i); return u8; };
  })();
  self.fetch = (orig => function(resource, init) {
    try {
      const name = typeof resource === 'string' ? resource.split('/').pop() : '';
      if (name && self.__WASM_B64_MAP && self.__WASM_B64_MAP[name]) {
        const bytes = self.__b64ToU8(self.__WASM_B64_MAP[name]);
        return Promise.resolve(new Response(bytes, { headers: { 'Content-Type': 'application/wasm' } }));
      }
    } catch (_e) {}
    return orig.call(this, resource, init);
  })(self.fetch);
  // Placeholder replaced by build.sh
  self.__WASM_B64_MAP['tab_coordinator_shared_worker_bg.wasm'] = '__WASM_B64_TC__';
})();

// Save the real onconnect handler
let realOnconnect = null;

// Set up onconnect immediately
self.onconnect = (e) => {
    console.log("JS onconnect fired");
    if (realOnconnect) {
        realOnconnect(e);
    } else {
        console.log("WASM not ready yet, connection will be handled after init");
        // Store the event to handle after WASM init
        self._pendingConnection = e;
    }
};

// Initialize WASM
wasm_bindgen("./tab_coordinator_shared_worker_bg.wasm")
    .then(() => {
        console.log("Worker WASM loaded");
        wasm_bindgen.main();
        
        // Store the real handler
        realOnconnect = wasm_bindgen.handle_connect;
        
        // Handle any pending connection
        if (self._pendingConnection) {
            console.log("Handling pending connection");
            realOnconnect(self._pendingConnection);
            self._pendingConnection = null;
        }
    })
    .catch(console.error); 