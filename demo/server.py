from http.server import HTTPServer, SimpleHTTPRequestHandler

class CORSRequestHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        SimpleHTTPRequestHandler.end_headers(self)

if __name__ == '__main__':
    server = HTTPServer(('localhost', 8080), CORSRequestHandler)
    print("Server started at http://localhost:8080")
    server.serve_forever() 