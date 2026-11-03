import { str } from "./validate";

// Dodo's webhook payloads, turned into the few typed events dotsafe acts on.
// This is the one place the untrusted JSON is read: a signed event that's
// missing what we need comes back as "malformed" (logged and acknowledged)
// rather than throwing halfway through and being retried forever.

export type DotsafeEvent =
  | {
      kind: "payment";
      paymentId: string;
      customerId: string;
      email: string;
      name: string | null;
      amount: number;
      currency: string;
      productIds: string[];
      at: string;
    }
  | { kind: "refund"; paymentId: string; refundId: string | null; amount: number | null; at: string }
  | { kind: "dispute"; paymentId: string; disputeId: string | null; at: string }
  | { kind: "other" }
  | { kind: "malformed" };

type Raw = { type?: unknown; timestamp?: unknown; data?: unknown };

const obj = (v: unknown): Record<string, unknown> => (v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : {});
const int = (v: unknown) => (typeof v === "number" && Number.isInteger(v) ? v : null);
const id = (v: unknown) => str(v, 200);

export function parseEvent(raw: Raw): DotsafeEvent {
  const d = obj(raw.data);
  const at = str(d.created_at, 64) ?? str(raw.timestamp, 64) ?? new Date().toISOString();

  switch (raw.type) {
    case "payment.succeeded": {
      const customer = obj(d.customer);
      const paymentId = id(d.payment_id);
      const customerId = id(customer.customer_id);
      const email = str(customer.email, 254);
      const amount = int(d.total_amount);
      const currency = str(d.currency, 8);
      if (!paymentId || !customerId || !email || amount === null || !currency) return { kind: "malformed" };
      const cart = Array.isArray(d.product_cart) ? d.product_cart : [];
      const productIds = cart.map((item) => id(obj(item).product_id)).filter((p): p is string => p !== null);
      return { kind: "payment", paymentId, customerId, email, name: str(customer.name, 200), amount, currency, productIds, at };
    }
    case "refund.succeeded": {
      const paymentId = id(d.payment_id);
      if (!paymentId) return { kind: "malformed" };
      return { kind: "refund", paymentId, refundId: id(d.refund_id), amount: int(d.amount), at };
    }
    case "dispute.opened": {
      const paymentId = id(d.payment_id);
      if (!paymentId) return { kind: "malformed" };
      return { kind: "dispute", paymentId, disputeId: id(d.dispute_id), at };
    }
    default:
      return { kind: "other" };
  }
}
