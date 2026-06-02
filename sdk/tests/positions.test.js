const assert = require('node:assert/strict');
const test = require('node:test');

const { reconstructOpenOrders, reconstructPositions } = require('../dist');

test('reconstructOpenOrders reports unfilled remainder and skips cancelled/filled', () => {
  const orders = [
    { orderId: 'a', marketId: 1, side: 'buy', price: 100n, size: 10n },
    { orderId: 'b', marketId: 1, side: 'sell', price: 110n, size: 5n, status: 'cancelled' },
    { orderId: 'c', marketId: 2, side: 'buy', price: 50n, size: 4n },
  ];
  const fills = [
    { orderId: 'a', marketId: 1, side: 'buy', price: 100n, size: 3n },
    { orderId: 'c', marketId: 2, side: 'buy', price: 50n, size: 4n }, // fully filled
  ];
  const open = reconstructOpenOrders({ orders, fills });
  assert.equal(open.length, 1);
  assert.equal(open[0].orderId, 'a');
  assert.equal(open[0].remaining, 7n);
});

test('reconstructPositions accumulates a long and computes weighted entry', () => {
  const fills = [
    { orderId: 'a', marketId: 1, side: 'buy', price: 100n, size: 10n },
    { orderId: 'b', marketId: 1, side: 'buy', price: 120n, size: 10n },
  ];
  const [pos] = reconstructPositions({ fills });
  assert.equal(pos.marketId, 1);
  assert.equal(pos.netSize, 20n);
  assert.equal(pos.entryPrice, 110n); // (100*10 + 120*10)/20
  assert.equal(pos.realizedPnl, 0n);
});

test('reconstructPositions realizes PnL on partial close of a long', () => {
  const fills = [
    { orderId: 'a', marketId: 1, side: 'buy', price: 100n, size: 10n },
    { orderId: 'b', marketId: 1, side: 'sell', price: 130n, size: 4n },
  ];
  const [pos] = reconstructPositions({ fills });
  assert.equal(pos.netSize, 6n);
  assert.equal(pos.entryPrice, 100n); // remaining long keeps original entry
  assert.equal(pos.realizedPnl, (130n - 100n) * 4n); // 120
});

test('reconstructPositions realizes PnL on a short and closing buy', () => {
  const fills = [
    { orderId: 'a', marketId: 7, side: 'sell', price: 200n, size: 5n },
    { orderId: 'b', marketId: 7, side: 'buy', price: 180n, size: 5n },
  ];
  const [pos] = reconstructPositions({ fills });
  assert.equal(pos.netSize, 0n);
  assert.equal(pos.entryPrice, 0n);
  assert.equal(pos.realizedPnl, (200n - 180n) * 5n); // short profit 100
});

test('reconstructPositions handles a flip from long to short', () => {
  const fills = [
    { orderId: 'a', marketId: 1, side: 'buy', price: 100n, size: 5n },
    { orderId: 'b', marketId: 1, side: 'sell', price: 120n, size: 8n },
  ];
  const [pos] = reconstructPositions({ fills });
  // closed 5 long @ +20 each = 100 realized, then 3 short opened @ 120
  assert.equal(pos.netSize, -3n);
  assert.equal(pos.entryPrice, 120n);
  assert.equal(pos.realizedPnl, 100n);
});

test('reconstructPositions separates markets and sorts by marketId', () => {
  const fills = [
    { orderId: 'a', marketId: 3, side: 'buy', price: 10n, size: 1n },
    { orderId: 'b', marketId: 1, side: 'buy', price: 20n, size: 2n },
  ];
  const positions = reconstructPositions({ fills });
  assert.deepEqual(positions.map((p) => p.marketId), [1, 3]);
});

test('negative sizes are rejected', () => {
  assert.throws(() => reconstructOpenOrders({ orders: [{ orderId: 'x', marketId: 1, side: 'buy', price: 1n, size: -1n }] }), /non-negative/);
  assert.throws(() => reconstructPositions({ fills: [{ orderId: 'x', marketId: 1, side: 'buy', price: 1n, size: -1n }] }), /non-negative/);
});
