/** Supplier orders from receipts (docs/green-coffee.md, "Orders"). */

import { BASE_URL, errorMessage, request } from '#lib/api/http.js';

export type OrderStatus = 'ordered' | 'shipped' | 'received' | 'cancelled';

export interface PurchaseOrderLine {
	id: string;
	position: number;
	kind: 'coffee' | 'other';
	name: string;
	sku: string | null;
	variant_id: string | null;
	bag: string | null;
	quantity: number;
	weight_g: number | null;
	/** The line's price, all of it. */
	price: number;
	taxed: boolean | null;
	url: string | null;
	bean_id: string | null;
	bean_name: string | null;
	wanted_id: string | null;
	move_id: string | null;
	/** What arrived, when it differed. */
	received_g: number | null;
}

/** What is kept of a receipt; the file itself is private. */
export interface Receipt {
	id: string;
	order_id: string;
	kind: 'confirmation' | 'shipping' | 'invoice' | 'other';
	content_type: string;
	file_name: string | null;
	message_id: string | null;
	sha256: string;
	size_bytes: number;
	added_at: string;
}

export interface PurchaseOrder {
	id: string;
	supplier: string;
	order_number: string;
	placed_at: string;
	status: OrderStatus;
	currency: string | null;
	subtotal: number | null;
	discount: number;
	shipping: number;
	tax: number;
	total: number;
	mismatch_reason: string | null;
	carrier: string | null;
	tracking: string | null;
	shipped_at: string | null;
	received_at: string | null;
	note: string | null;
	created_at: string;
	updated_at: string;
	lines: PurchaseOrderLine[];
	receipts: Receipt[];
	coffee_g: number;
}

export const orders = {
	/** open (ordered or shipped), all, or one status */
	list: (status: 'open' | 'all' | OrderStatus = 'open') =>
		request<PurchaseOrder[]>(`/api/orders?status=${status}`),
	get: (id: string) => request<PurchaseOrder>(`/api/orders/${encodeURIComponent(id)}`),
	shipped: (id: string, req: { carrier?: string; tracking?: string; at?: string }) =>
		request<PurchaseOrder>(`/api/orders/${encodeURIComponent(id)}/shipped`, {
			method: 'POST',
			body: JSON.stringify(req)
		}),
	received: (
		id: string,
		req: { at?: string; lines?: { line: string; weight_g?: number; bean_id?: string }[] }
	) =>
		request<PurchaseOrder>(`/api/orders/${encodeURIComponent(id)}/received`, {
			method: 'POST',
			body: JSON.stringify(req)
		}),
	cancel: (id: string) =>
		request<PurchaseOrder>(`/api/orders/${encodeURIComponent(id)}/cancel`, { method: 'POST' }),
	remove: (id: string) =>
		request<void>(`/api/orders/${encodeURIComponent(id)}`, { method: 'DELETE' }),
	receiptPath: (id: string, receiptId: string) =>
		`/api/orders/${encodeURIComponent(id)}/receipts/${encodeURIComponent(receiptId)}`,
	/** Keep a receipt with its order: the file is the body */
	addReceipt: async (
		id: string,
		file: File,
		kind: Receipt['kind'] = 'confirmation'
	): Promise<Receipt> => {
		const q = `?kind=${kind}&file_name=${encodeURIComponent(file.name)}`;
		const res = await fetch(`${BASE_URL}/api/orders/${encodeURIComponent(id)}/receipts${q}`, {
			method: 'POST',
			credentials: 'same-origin',
			headers: { 'Content-Type': file.type || 'application/octet-stream' },
			body: file
		});
		if (res.status === 401) {
			window.dispatchEvent(new CustomEvent('auth_required'));
			throw new Error('Authentication required');
		}
		if (!res.ok) throw new Error(errorMessage(await res.text()));
		return res.json();
	},
	removeReceipt: (id: string, receiptId: string) =>
		request<void>(
			`/api/orders/${encodeURIComponent(id)}/receipts/${encodeURIComponent(receiptId)}`,
			{ method: 'DELETE' }
		)
};
