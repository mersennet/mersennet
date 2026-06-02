/**
 * Client-side open-order and position reconstruction (Workstream F5,
 * `orders:read` / `positions:read`).
 *
 * On a fully shielded chain the node never sees order/position plaintext,
 * so a wallet reconstructs its own trading state locally from material it
 * already holds:
 *
 *  - `OrderRecord`s it created when it submitted each shielded order
 *    (it knows side/price/size/market because it authored the intent), and
 *  - `FillRecord`s it derived by matching its orders against the public
 *    frequent-batch-auction clearing events (`FbaCleared` / WS
 *    `newClearingPrice`) plus its own collateral spend graph.
 *
 * These helpers are pure and deterministic so they unit-test cleanly and
 * run in the browser with no server round-trip. They do not require the
 * (separate, consensus-level) work of having the chain mint per-trader
 * encrypted fill notes; a wallet that tracks its own submissions and reads
 * public clearing data has everything it needs.
 */

export type OrderSide = 'buy' | 'sell';

/** An order the wallet submitted, as it recorded it at submit time. */
export interface OrderRecord {
  orderId: string;
  marketId: number;
  side: OrderSide;
  /** Limit price in the market's smallest price units. */
  price: bigint;
  /** Original order size in the market's smallest size units. */
  size: bigint;
  /** Terminal state the wallet already knows locally, if any. */
  status?: 'open' | 'cancelled';
}

/** A fill the wallet attributed to one of its orders from public clearing. */
export interface FillRecord {
  orderId: string;
  marketId: number;
  side: OrderSide;
  /** Clearing price the fill executed at. */
  price: bigint;
  /** Filled size (<= the order's remaining size). */
  size: bigint;
}

export interface OpenOrder {
  orderId: string;
  marketId: number;
  side: OrderSide;
  price: bigint;
  /** Unfilled size still resting in the book. */
  remaining: bigint;
}

export interface ReconstructedPosition {
  marketId: number;
  /** Signed net size: positive = long, negative = short, 0 = flat. */
  netSize: bigint;
  /** Size-weighted average entry price of the currently-open exposure. */
  entryPrice: bigint;
  /** Realized PnL accumulated from closing/reducing exposure. */
  realizedPnl: bigint;
}

export interface ReconstructTradingOptions {
  orders: OrderRecord[];
  fills?: FillRecord[];
}

function sumFilledByOrder(fills: FillRecord[]): Map<string, bigint> {
  const filled = new Map<string, bigint>();
  for (const f of fills) {
    if (f.size < 0n) {
      throw new Error(`fill size must be non-negative (order ${f.orderId})`);
    }
    filled.set(f.orderId, (filled.get(f.orderId) ?? 0n) + f.size);
  }
  return filled;
}

/**
 * Reconstruct the wallet's still-open orders: every non-cancelled order
 * whose filled size is below its original size, with the unfilled
 * remainder reported.
 */
export function reconstructOpenOrders(options: ReconstructTradingOptions): OpenOrder[] {
  const fills = options.fills ?? [];
  const filled = sumFilledByOrder(fills);

  const open: OpenOrder[] = [];
  for (const order of options.orders) {
    if (order.status === 'cancelled') {
      continue;
    }
    if (order.size < 0n) {
      throw new Error(`order size must be non-negative (order ${order.orderId})`);
    }
    const done = filled.get(order.orderId) ?? 0n;
    const remaining = order.size - done;
    if (remaining > 0n) {
      open.push({
        orderId: order.orderId,
        marketId: order.marketId,
        side: order.side,
        price: order.price,
        remaining,
      });
    }
  }
  return open;
}

interface MutablePosition {
  netSize: bigint; // signed
  entryPrice: bigint; // avg entry of open exposure (non-negative)
  realizedPnl: bigint;
}

/**
 * Apply one signed fill to a running position using average-cost
 * accounting. Increasing exposure updates the weighted entry price;
 * reducing or flipping realizes PnL on the closed portion.
 */
function applyFill(pos: MutablePosition, signedSize: bigint, price: bigint): void {
  const sameDirection =
    pos.netSize === 0n || (pos.netSize > 0n) === (signedSize > 0n);

  if (sameDirection) {
    const newNet = pos.netSize + signedSize;
    const prevAbs = pos.netSize < 0n ? -pos.netSize : pos.netSize;
    const addAbs = signedSize < 0n ? -signedSize : signedSize;
    const totalAbs = prevAbs + addAbs;
    pos.entryPrice = totalAbs === 0n
      ? 0n
      : (pos.entryPrice * prevAbs + price * addAbs) / totalAbs;
    pos.netSize = newNet;
    return;
  }

  // Opposite direction: close against existing exposure first.
  const closingAbs = signedSize < 0n ? -signedSize : signedSize;
  const openAbs = pos.netSize < 0n ? -pos.netSize : pos.netSize;
  const matched = closingAbs < openAbs ? closingAbs : openAbs;

  // Realized PnL on the matched (closed) portion.
  // Long close: (exit - entry) * matched. Short close: (entry - exit) * matched.
  const wasLong = pos.netSize > 0n;
  const pnl = wasLong
    ? (price - pos.entryPrice) * matched
    : (pos.entryPrice - price) * matched;
  pos.realizedPnl += pnl;

  const remainderAbs = closingAbs - matched;
  pos.netSize = pos.netSize + signedSize;

  if (pos.netSize === 0n) {
    pos.entryPrice = 0n;
  } else if (remainderAbs > 0n) {
    // Position flipped to the opposite side; the leftover opens new
    // exposure at the fill price.
    pos.entryPrice = price;
  }
  // If it only reduced (remainderAbs == 0 and netSize != 0), entryPrice
  // of the remaining open exposure is unchanged.
}

/**
 * Reconstruct per-market positions from fills using average-cost
 * accounting. Fills are applied in array order, so pass them
 * chronologically for correct realized-PnL attribution.
 */
export function reconstructPositions(options: ReconstructTradingOptions): ReconstructedPosition[] {
  const fills = options.fills ?? [];
  const byMarket = new Map<number, MutablePosition>();

  for (const f of fills) {
    if (f.size < 0n) {
      throw new Error(`fill size must be non-negative (order ${f.orderId})`);
    }
    const pos = byMarket.get(f.marketId) ?? {
      netSize: 0n,
      entryPrice: 0n,
      realizedPnl: 0n,
    };
    const signed = f.side === 'buy' ? f.size : -f.size;
    applyFill(pos, signed, f.price);
    byMarket.set(f.marketId, pos);
  }

  return Array.from(byMarket.entries())
    .sort((a, b) => a[0] - b[0])
    .map(([marketId, pos]) => ({
      marketId,
      netSize: pos.netSize,
      entryPrice: pos.entryPrice,
      realizedPnl: pos.realizedPnl,
    }));
}
