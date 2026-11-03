// Gives someone a free dotsafe license: the API issues a key, stores its hash
// and emails it, as for a purchase. Prints the key once.
//
//   DOTSAFE_ADMIN_TOKEN=… bun run license:create <email> [name]
//
// DOTSAFE_ADMIN_TOKEN is the ADMIN_TOKEN secret set on the dotsafe-api Worker.
// DOTSAFE_API_URL points it at another deployment (default https://api.dotsafe.dev).

const [email, ...nameParts] = process.argv.slice(2);
const name = nameParts.join(" ").trim() || undefined;
const token = process.env.DOTSAFE_ADMIN_TOKEN;
const api = process.env.DOTSAFE_API_URL ?? "https://api.dotsafe.dev";

if (!email) {
  console.error("usage: bun run license:create <email> [name]");
  process.exit(2);
}
if (!token) {
  console.error("Set DOTSAFE_ADMIN_TOKEN to the ADMIN_TOKEN secret of the dotsafe-api Worker.");
  process.exit(2);
}

const res = await fetch(`${api}/v1/admin/licenses`, {
  method: "POST",
  headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
  body: JSON.stringify({ email, name }),
});
const body = (await res.json().catch(() => null)) as any;
if (!res.ok) {
  console.error(`Failed (${res.status}): ${body?.error?.message ?? "no details"}`);
  process.exit(1);
}
console.log(`License ${body.license_id} for ${email}, emailed.`);
console.log(`Key: ${body.key}`);
