import {createServer} from 'node:http';
import {createReadStream} from 'node:fs';
import {stat} from 'node:fs/promises';
import {resolve, sep} from 'node:path';

const [rootArgument, portArgument] = process.argv.slice(2);
if (!rootArgument || !portArgument) throw new Error('Expected root and port');
const root = resolve(rootArgument);
createServer(async (request, response) => {
  const pathname = new URL(request.url, 'http://localhost').pathname;
  const target = resolve(root, `.${decodeURIComponent(pathname)}`);
  if (!target.startsWith(root + sep)) { response.writeHead(403).end(); return; }
  try {
    const info = await stat(target);
    if (!info.isFile()) { response.writeHead(404).end(); return; }
    response.writeHead(200, {'content-length': info.size, 'content-type': 'application/octet-stream'});
    createReadStream(target).pipe(response);
  } catch { response.writeHead(404).end(); }
}).listen(Number(portArgument), '127.0.0.1');
