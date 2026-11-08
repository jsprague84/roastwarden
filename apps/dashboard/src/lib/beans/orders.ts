/** Supplier orders from receipts (docs/green-coffee.md, "Orders"): what the Beans pages say about them. */
import type { PurchaseOrderLine, OrderStatus, PurchaseOrder } from '#lib/api/orders.js';

/** "Sweet Maria's 000412345" */
export function orderName(o: Pick<PurchaseOrder, 'supplier' | 'order_number'>): string {
	return `${o.supplier} ${o.order_number}`;
}

/** Ordered or shipped: not in stock yet. */
export function isOpen(o: Pick<PurchaseOrder, 'status'>): boolean {
	return o.status === 'ordered' || o.status === 'shipped';
}

const STATUS: Record<OrderStatus, string> = {
	ordered: 'Ordered',
	shipped: 'Shipped',
	received: 'Received',
	cancelled: 'Cancelled'
};

export function statusText(s: OrderStatus): string {
	return STATUS[s] ?? s;
}

/** The order's lines add up to its total: lines − discount + shipping + tax, to the cent. */
export function totals(
	o: Pick<PurchaseOrder, 'lines' | 'discount' | 'shipping' | 'tax' | 'total'>
): {
	lines: number;
	computed: number;
	difference: number;
	addsUp: boolean;
} {
	const cents = (v: number) => Math.round(v * 100) / 100;
	const lines = cents(o.lines.reduce((a, l) => a + l.price, 0));
	const computed = cents(lines - o.discount + o.shipping + o.tax);
	const difference = cents(o.total - computed);
	return { lines, computed, difference, addsUp: Math.abs(difference) < 0.005 };
}

/** The "received" call's lines: only those whose weight was changed from what was ordered. */
export function receivedLines(
	lines: PurchaseOrderLine[],
	typed: Record<string, number | null | undefined>
): { line: string; weight_g: number }[] {
	const out: { line: string; weight_g: number }[] = [];
	for (const l of lines) {
		if (l.kind !== 'coffee') continue;
		const g = typed[l.id];
		if (g == null || !Number.isFinite(g) || g < 0) continue;
		if (l.weight_g != null && Math.abs(g - l.weight_g) < 0.05) continue;
		out.push({ line: String(l.position), weight_g: g });
	}
	return out;
}

/** "2 coffees, 1 other" */
export function lineSummary(lines: Pick<PurchaseOrderLine, 'kind'>[]): string {
	const coffee = lines.filter((l) => l.kind === 'coffee').length;
	const other = lines.length - coffee;
	const c = `${coffee} ${coffee === 1 ? 'coffee' : 'coffees'}`;
	return other ? `${c}, ${other} other` : c;
}

/** "12 KB" */
export function fileSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
	return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
