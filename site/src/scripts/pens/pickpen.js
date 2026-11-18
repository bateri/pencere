// pickpen, choosing rows in the middle of a pipeline, drawn with real elements after
// pickpen/docs/confirm.png. The view opens with the rows `--where` matched already marked, one of
// them unmarked by hand: a rule marks, a person confirms. A press on a row changes its mark; the
// Output panel shows the rows that will go and the command that does the same without asking. Send
// sends the rows on down the pipeline; Send nothing sends none. Taken from the bateri landing page
// (src/scripts/pens/pickpen.js). The pods are sample data.
import { h, ICON, kbd, acts } from "./kit.js";

const PODS = `api-7431ef2f0c-dtgxr 1/1 Running 0 53m
api-7431ef2f0c-5ppws 0/1 Completed 0 20m
web-2883e51d31-tspq7 0/1 CrashLoopBackOff 22_(6m_ago) 23m
web-2883e51d31-b6kgl 1/1 Running 0 40m
web-2883e51d31-2gzbq 1/1 Running 3_(6h_ago) 1d3h
web-2883e51d31-j6n2c 1/1 Running 1_(3h_ago) 8d14h
worker-3cbf0eec06-z5msl 0/1 Error 0 12m
worker-3cbf0eec06-hn77j 1/1 Running 2_(2h_ago) 7d7h
worker-3cbf0eec06-9gvbn 1/1 Running 0 52m
worker-3cbf0eec06-tv98g 0/1 Error 0 7d14h
billing-2e71d56062-7wsgm 1/1 Running 0 16m
billing-2e71d56062-4kt2f 0/1 CrashLoopBackOff 33_(6m_ago) 4d6h
search-306b8f6ab1-wlqjm 1/1 Running 3_(3h_ago) 8d17h
search-306b8f6ab1-rrqjp 0/1 Error 0 6d3h
auth-0db503989e-l99k7 1/1 Running 2_(5h_ago) 6d22h
auth-0db503989e-z4cc4 1/1 Running 0 2d11h
auth-0db503989e-5w667 0/1 Completed 0 47m
auth-0db503989e-h6wxm 1/1 Running 0 47m
mailer-1a3d5d1817-62dq7 0/1 CrashLoopBackOff 17_(2m_ago) 9d9h
mailer-1a3d5d1817-r6txh 1/1 Running 0 4d1h
mailer-1a3d5d1817-mx8f8 1/1 Running 0 3d4h
scheduler-5293a8bf01-gk6d2 1/1 Running 0 2d16h
scheduler-5293a8bf01-p29fs 1/1 Running 0 4d6h
scheduler-5293a8bf01-dcl4z 0/1 CrashLoopBackOff 22_(5m_ago) 4m
cache-83eb058819-26fss 0/1 CrashLoopBackOff 5_(4m_ago) 41m
cache-83eb058819-8krzt 0/1 Completed 0 15m
cache-83eb058819-m2xqv 1/1 Running 1_(14h_ago) 7d7h`
  .split("\n")
  .map((l) => {
    const [name, ready, status, restarts, age] = l.split(" ");
    return { name, ready, status, restarts: restarts.replace(/_/g, " "), age };
  });

export function pickpen(stage) {
  // `--where 'STATUS!=Running'` marks these; it opens with one of them unmarked by hand.
  const matches = PODS.filter((p) => p.status !== "Running").map((p) => p.name);
  const marked = new Set(matches.filter((n) => n !== "web-2883e51d31-tspq7"));
  let cursor = "web-2883e51d31-b6kgl";

  const el = h(`
  <div class="pn pn--pick">
    <div class="pk-title"><span class="pn-acc">${ICON.list}</span><b>Delete these pods?</b></div>
    <div class="pk-query">
      <span class="pn-acc">${ICON.right}</span><span class="pn-dim pn-cut">Type to filter: words, !word, STATUS=Running, AGE&lt;1h — F1 for more</span>
      <span class="pn-sp"></span><span class="pn-dim">${PODS.length} rows ·</span><span class="pn-acc" data-n></span>
    </div>
    <div class="pk-in">
      <div class="pk-table">
        <div class="pk-row pk-head"><span><i class="pk-box" data-all></i></span><span>NAME</span><span>READY</span><span>STATUS</span><span>RESTARTS</span><span class="pn-r">AGE</span></div>
        <div class="pk-rows" data-rows></div>
      </div>
      <div class="pk-out">
        <div class="pk-tabs"><span>Row ${kbd("F2")}</span><span>Column ${kbd("F3")}</span><span class="is-on">Output ${kbd("F4")}</span><span class="pn-sp"></span><span class="pn-dim">${ICON.close}</span></div>
        <div class="pk-sec">
          <b class="pn-dim">Again, with nobody to ask</b>
          <div class="pk-cmd"><span>pickpen --where 'STATUS!=Running' --field NAME</span><span class="pn-dim">${ICON.copy}</span></div>
          <p class="pn-dim" data-note>${ICON.info}<span data-note-text></span></p>
        </div>
        <div class="pk-sec pk-sec--out">
          <b class="pn-dim">What goes out <span data-out-n></span></b>
          <span class="pk-chip">NAME ${ICON.close}</span>
          <div class="pk-list" data-list></div>
        </div>
      </div>
    </div>
    <div class="pn-keys pk-keys">
      <span>${kbd("⇥")}mark</span><span>${kbd("⏎")}send</span><span class="pk-k">${kbd("Esc")}back</span><span class="pk-k">${kbd("⌘K")}commands</span><span class="pk-k">${kbd("F1")}help</span>
      <span class="pn-sp"></span>
      <button type="button" class="pk-none" data-act="none">${ICON.back}Send nothing</button>
      <button type="button" class="pn-btn pn-btn--go" data-act="send">${ICON.send}<span data-send></span>${kbd("⏎")}</button>
    </div>
  </div>`);

  const rows = el.querySelector("[data-rows]");
  const rowsWord = (n) => `${n} row${n === 1 ? "" : "s"}`;
  function paint() {
    rows.innerHTML = PODS.map((p) => {
      const on = marked.has(p.name);
      return `<button type="button" role="checkbox" aria-checked="${on}" class="pk-row${on ? " is-on" : ""}${p.name === cursor ? " is-at" : ""}" data-act="mark" data-name="${p.name}"><span><i class="pk-box">${ICON.check}</i></span><span class="pn-cut">${p.name}</span><span>${p.ready}</span><span class="pn-cut">${p.status}</span><span class="pn-cut">${p.restarts}</span><span class="pn-r">${p.age}</span></button>`;
    }).join("");
    const n = marked.size,
      out = PODS.filter((p) => marked.has(p.name));
    el.querySelector("[data-n]").textContent = `${n} marked`;
    el.querySelector("[data-all]").className =
      `pk-box ${n === 0 ? "" : n === PODS.length ? "is-all" : "is-some"}`;
    el.querySelector("[data-out-n]").textContent = `· ${rowsWord(n)}`;
    el.querySelector("[data-list]").innerHTML = out.map((p) => `<span>${p.name}</span>`).join("");
    el.querySelector("[data-send]").textContent = `Send ${rowsWord(n)}`;
    el.querySelector("[data-act=send]").disabled = n === 0;
    // The command chooses the same rows only when the marked ones are the ones the query matched.
    const same = n === matches.length && matches.every((m) => marked.has(m));
    el.querySelector("[data-note]").hidden = same;
    el.querySelector("[data-note-text]").textContent =
      `Would send the ${matches.length} rows it matches; ${n} ${n === 1 ? "is" : "are"} marked, by hand.`;
  }
  paint();

  acts(el, {
    mark: (b) => {
      const name = b.dataset.name,
        top = rows.scrollTop;
      cursor = name;
      marked.has(name) ? marked.delete(name) : marked.add(name);
      paint();
      rows.scrollTop = top;
      rows.querySelector(`[data-name="${name}"]`)?.focus({ preventScroll: true });
    },
    // The rows go to stdout; the next command (xargs kubectl delete pod) writes a line for each.
    send: () =>
      stage.close(
        PODS.filter((p) => marked.has(p.name)).map((p) => [[`pod "${p.name}" deleted`, "fg"]]),
      ),
    none: () => stage.close(),
  });
  return el;
}
