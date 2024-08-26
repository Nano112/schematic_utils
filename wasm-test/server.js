import { serve } from "bun";
import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

const server = serve({
    port: 3000,
    fetch(req) {
        const url = new URL(req.url);
        let path = url.pathname;

        if (path === "/") path = "/index.html";

        const filePath = join(__dirname, path);

        try {
            const content = readFileSync(filePath);
            const contentType = path.endsWith('.html') ? 'text/html' :
                path.endsWith('.js') ? 'application/javascript' :
                    path.endsWith('.wasm') ? 'application/wasm' : 'text/plain';

            return new Response(content, {
                headers: {
                    "Content-Type": contentType,
                    "Cross-Origin-Opener-Policy": "same-origin",
                    "Cross-Origin-Embedder-Policy": "require-corp"
                }
            });
        } catch (error) {
            return new Response("Not Found", { status: 404 });
        }
    },
});

console.log(`Listening on http://localhost:${server.port}...`);