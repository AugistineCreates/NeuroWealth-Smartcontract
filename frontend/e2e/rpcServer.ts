import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';

export interface RpcRequestRecord {
  scenario: string;
  method: string;
  params: Record<string, unknown>;
}

export interface LocalRpcServer {
  url: string;
  requests: RpcRequestRecord[];
  close(): Promise<void>;
}

export async function startLocalRpcServer(): Promise<LocalRpcServer> {
  const requests: RpcRequestRecord[] = [];
  const server: Server = createServer((request, response) => {
    response.setHeader('Access-Control-Allow-Origin', '*');
    response.setHeader('Access-Control-Allow-Headers', 'content-type');
    response.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
    if (request.method === 'OPTIONS') {
      response.writeHead(204).end();
      return;
    }

    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk: string) => { body += chunk; });
    request.on('end', () => {
      void (async () => {
        const payload = JSON.parse(body) as RpcRequestRecord;
        requests.push(payload);
        const { method, params, scenario } = payload;
        let result: unknown;

        if (method === 'connectWallet') result = 'GABCDE123456789WXYZ';
        else if (method === 'getNetworkDetails') {
          result = { networkPassphrase: scenario === 'wrong-network' ? 'Wrong Network' : 'Test SDF Network ; September 2015' };
        } else if (method === 'signTransaction' && scenario === 'rejected') {
          response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ error: 'User rejected signature' }));
          return;
        } else if (method === 'simulateContractCall') {
          const values: Record<string, unknown> = {
            get_balance: 1_000_000_000,
            get_user_strategy: 'balanced',
            get_exchange_rate: 12_500_000,
            get_total_assets: 1_250_000_000,
            get_total_shares: 1_000_000_000,
          };
          result = values[String(params.method)];
        } else if (method === 'simulateTransaction' && scenario === 'simulation-failed') {
          response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ error: 'Transaction simulation failed' }));
          return;
        } else if (method === 'simulateTransaction') result = { xdr: 'local-test-xdr' };
        else if (method === 'signTransaction') result = `signed:${String(params.xdr)}`;
        else if (method === 'sendTransaction') result = { status: 'PENDING', hash: 'local-test-tx-hash-0001' };
        else if (method === 'getTransaction') {
          if (scenario !== 'pending') await new Promise((resolve) => setTimeout(resolve, 500));
          result = { status: scenario === 'pending' ? 'NOT_FOUND' : 'SUCCESS' };
        } else result = {};

        response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ result }));
      })().catch((error: unknown) => {
        response.writeHead(500, { 'content-type': 'application/json' }).end(JSON.stringify({ error: String(error) }));
      });
    });
  });

  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address() as AddressInfo;
  return {
    url: `http://127.0.0.1:${address.port}/rpc`,
    requests,
    close: () => new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve())),
  };
}