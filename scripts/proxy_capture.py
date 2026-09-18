"""
Echo proxy: logs request and returns a fake streaming response.
No forwarding to opencode.ai — just captures what the client sends.
"""
from http.server import HTTPServer, BaseHTTPRequestHandler
import json, sys, datetime, os, io, time

LOG = os.path.join(os.path.dirname(__file__), "capture.log")

def log(msg):
    with open(LOG, "a", encoding="utf-8") as f:
        f.write(msg + "\n")
    print(msg, flush=True)

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
        log(f"\n{'='*70}")
        log(f"[{ts}] >>> POST {self.path}")
        log("--- Headers ---")
        for k, v in sorted(self.headers.items()):
            log(f"  {k}: {v}")
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length) if length else b""
        if body:
            log(f"--- Body ({len(body)} bytes) ---")
            try:
                parsed = json.loads(body)
                log(json.dumps(parsed, indent=2)[:5000])
            except:
                log(body.decode("utf-8", errors="replace")[:5000])

        # Return fake streaming SSE response
        log(f"[{ts}] <<< Fake response")
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()

        chunks = [
            'data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":0,"model":"test","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}\n\n',
            'data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":0,"model":"test","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}\n\n',
            'data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":0,"model":"test","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\n',
            'data: [DONE]\n\n',
        ]
        for chunk in chunks:
            self.wfile.write(chunk.encode())
            self.wfile.flush()
        sys.stdout.flush()

    def do_GET(self):
        # Handle /models endpoint
        ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
        log(f"\n[{ts}] >>> GET {self.path}")
        for k, v in sorted(self.headers.items()):
            log(f"  {k}: {v}")

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        resp = {"data": [{"id": "test-model", "object": "model", "owned_by": "test"}]}
        self.wfile.write(json.dumps(resp).encode())

    def do_OPTIONS(self):
        self.send_response(200)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.end_headers()

    def log_message(self, *a): pass

if __name__ == "__main__":
    if os.path.exists(LOG):
        os.remove(LOG)
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 18080
    srv = HTTPServer(("127.0.0.1", port), Handler)
    log(f"Echo proxy on http://127.0.0.1:{port}")
    srv.serve_forever()
