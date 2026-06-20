const state = {
  page: "api",
  detail: null,
  data: null,
  offset: 0,
  limit: 50,
  search: "",
  extra: {},
  live: true,
  refreshTimer: null,
};

const TAG_DEFS = {
  sniper: { label: "Sniper", color: "#FF0000" },
  blatant_cheater: { label: "Blatant Cheater", color: "#FFA500" },
  closet_cheater: { label: "Closet Cheater", color: "#FFA500" },
  confirmed_cheater: { label: "Confirmed Cheater", color: "#AF00AF" },
  replays_needed: { label: "Replays Needed", color: "#C0C0C0" },
  caution: { label: "Caution", color: "#C0C0C0" },
};

function getKey() {
  return new URLSearchParams(location.search).get("key") || "";
}

let inflight = 0;
function setLoading(on) {
  inflight = Math.max(0, inflight + (on ? 1 : -1));
  document.getElementById("loadbar")?.classList.toggle("on", inflight > 0);
}

async function api(path) {
  setLoading(true);
  try {
    const sep = path.includes("?") ? "&" : "?";
    const res = await fetch(`/api${path}${sep}key=${getKey()}`);
    if (!res.ok) {
      throw new Error(
        res.status === 401 || res.status === 403
          ? "Access denied — append ?key=<owner API key> to the URL"
          : `Request failed (${res.status})`,
      );
    }
    return await res.json();
  } finally {
    setLoading(false);
  }
}

function formatDate(iso) {
  if (!iso) return "-";
  return new Date(iso).toLocaleString();
}

const nameCache = { uuid: {}, discord: {} };

function idPair(attr, id, name, idText) {
  return `<span class="idpair" data-${attr}="${id}" title="${id}"><span class="idpair-name">${name ? esc(name) : ""}</span><span class="mono idpair-id">${idText}</span></span>`;
}

function formatDiscordId(id) {
  if (!id) return '<span class="text-muted">—</span>';
  const n = nameCache.discord[id];
  return idPair("discord", id, n ? "@" + n : "", String(id));
}

function formatUuid(uuid) {
  if (!uuid) return "-";
  return idPair("uuid", uuid, nameCache.uuid[uuid] || "", esc(uuid));
}

const inflightNames = { uuid: new Set(), discord: new Set() };

function resolveNames() {
  applyNames();
  resolveBatch("uuid");
  resolveBatch("discord");
}

async function resolveBatch(kind) {
  const store = nameCache[kind];
  const inflight = inflightNames[kind];
  const ids = [
    ...new Set([...document.querySelectorAll(`[data-${kind}]`)].map((e) => e.dataset[kind])),
  ].filter((v) => v && !(v in store) && !inflight.has(v));
  if (!ids.length) return;
  ids.forEach((v) => inflight.add(v));
  const param = kind === "uuid" ? "uuids" : "discord";
  const r = await api(`/resolve?${param}=${ids.join(",")}`).catch(() => null);
  ids.forEach((v) => inflight.delete(v));
  if (r) {
    ids.forEach((v) => (store[v] = r[kind === "uuid" ? "uuids" : "discord"]?.[v] ?? null));
    applyNames();
  }
}

function applyNames() {
  document.querySelectorAll("[data-uuid]").forEach((e) => {
    const n = nameCache.uuid[e.dataset.uuid];
    const slot = e.querySelector(".idpair-name");
    if (n && slot) slot.textContent = n;
  });
  document.querySelectorAll("[data-discord]").forEach((e) => {
    const n = nameCache.discord[e.dataset.discord];
    const slot = e.querySelector(".idpair-name");
    if (n && slot) slot.textContent = "@" + n;
  });
}

const accessRanks = ["Default", "Trusted", "Helper", "Moderator", "Admin", "Owner"];

function renderBadges(member) {
  const badges = [];
  if (member.is_owner) {
    badges.push('<span class="badge badge-admin">Owner</span>');
  } else {
    const level = member.access_level || 0;
    if (level > 0) {
      const cls = level >= 4 ? "badge-admin" : "badge-mod";
      badges.push(`<span class="badge ${cls}">${accessRanks[Math.min(level, 5)]}</span>`);
    }
  }
  if (member.key_locked)
    badges.push('<span class="badge badge-locked">Locked</span>');
  return badges.join(" ") || '<span class="text-muted">-</span>';
}

function renderTagBadge(tagType) {
  const def = TAG_DEFS[tagType];
  const color = def ? def.color : "#6b7280";
  const label = def ? def.label : tagType;
  return `<span class="tag-badge" style="--tag:${color}">${label}</span>`;
}

function navigate(page, detail = null) {
  clearTimeout(state.refreshTimer);
  state.page = page;
  state.detail = detail;
  state.offset = 0;
  state.search = "";
  state.extra = {};
  render();
  loadData();
}

function setParam(key, value) {
  if (value === "" || value === null) delete state.extra[key];
  else state.extra[key] = value;
  state.offset = 0;
  loadData();
}

function setOffset(offset) {
  state.offset = Math.max(0, offset);
  loadData();
}

async function loadData() {
  const main = document.getElementById("main");
  main.innerHTML = '<div class="loading">Loading...</div>';

  try {
    if (state.detail) {
      await loadDetail();
    } else {
      await loadList();
    }
    resolveNames();
  } catch (e) {
    main.innerHTML = `<div class="empty">Error: ${e.message}</div>`;
  }
}

async function loadList() {
  if (state.page === "api") return renderApiDashboard();

  const params = new URLSearchParams();
  params.set("limit", state.limit);
  params.set("offset", state.offset);
  if (state.search) params.set("search", state.search);
  for (const [k, v] of Object.entries(state.extra)) {
    if (v !== "" && v !== null) params.set(k, v);
  }

  state.data = await api(`/${state.page}?${params}`);
  renderList();
}

async function loadDetail() {
  if (state.page === "players") return loadPlayerView();
  if (state.page === "guilds") return loadGuildView();
  state.data = await api(`/${state.page}/${state.detail}`);
  renderDetail();
}

function renderList() {
  const main = document.getElementById("main");

  switch (state.page) {
    case "members":
      renderMembersList(main);
      break;
    case "blacklist":
      renderBlacklistList(main);
      break;
    case "players":
      renderPlayersList(main);
      break;
    case "guilds":
      renderGuildsList(main);
      break;
  }
}

function renderMembersList(main) {
  const { total, members } = state.data;

  main.innerHTML = `
        <div class="header">
            <h2>Members</h2>
            <div class="controls">
                <input type="text" id="search" placeholder="Search Discord ID or UUID..." value="${state.search}">
                <select onchange="setParam('sort', this.value)">
                    <option value="">Sort: ID</option>
                    <option value="requests" ${state.extra.sort === "requests" ? "selected" : ""}>Requests</option>
                    <option value="joined" ${state.extra.sort === "joined" ? "selected" : ""}>Joined</option>
                    <option value="access" ${state.extra.sort === "access" ? "selected" : ""}>Access</option>
                </select>
                <select onchange="setParam('dir', this.value)">
                    <option value="">Desc</option>
                    <option value="asc" ${state.extra.dir === "asc" ? "selected" : ""}>Asc</option>
                </select>
                <select onchange="setParam('rank', this.value)">
                    <option value="">Any rank</option>
                    <option value="1" ${state.extra.rank === "1" ? "selected" : ""}>Trusted+</option>
                    <option value="3" ${state.extra.rank === "3" ? "selected" : ""}>Mod+</option>
                    <option value="4" ${state.extra.rank === "4" ? "selected" : ""}>Admin+</option>
                </select>
                <label class="check"><input type="checkbox" ${state.extra.locked ? "checked" : ""} onchange="setParam('locked', this.checked ? 'true' : '')"> Locked</label>
                <label class="check"><input type="checkbox" ${state.extra.haskey ? "checked" : ""} onchange="setParam('haskey', this.checked ? 'true' : '')"> Has key</label>
                <button onclick="doSearch()">Search</button>
            </div>
        </div>
        <table>
            <thead>
                <tr>
                    <th>ID</th>
                    <th>Discord ID</th>
                    <th>UUID</th>
                    <th>Access</th>
                    <th>Requests</th>
                    <th>Joined</th>
                </tr>
            </thead>
            <tbody>
                ${members
                  .map(
                    (m) => `
                    <tr class="clickable" onclick="navigate('members', ${m.id})">
                        <td>${m.id}</td>
                        <td>${formatDiscordId(m.discord_id)}</td>
                        <td>${formatUuid(m.uuid)}</td>
                        <td>${renderBadges(m)}</td>
                        <td>${m.request_count.toLocaleString()}</td>
                        <td>${formatDate(m.join_date)}</td>
                    </tr>
                `,
                  )
                  .join("")}
            </tbody>
        </table>
        ${renderPagination(total)}
    `;

  document.getElementById("search").addEventListener("keypress", (e) => {
    if (e.key === "Enter") doSearch();
  });
}

function renderBlacklistList(main) {
  const { total, players } = state.data;
  const tagTypes = Object.entries(TAG_DEFS).map(([v, d]) => [v, d.label]);

  main.innerHTML = `
        <div class="header">
            <h2>Blacklist</h2>
            <div class="controls">
                <select onchange="setParam('field', this.value)">
                    <option value="">By UUID</option>
                    <option value="tagger" ${state.extra.field === "tagger" ? "selected" : ""}>By Tagger (Discord ID)</option>
                    <option value="reason" ${state.extra.field === "reason" ? "selected" : ""}>By Reason</option>
                </select>
                <input type="text" id="search" placeholder="Search..." value="${state.search}">
                <select onchange="setParam('tag_type', this.value)">
                    <option value="">All Tags</option>
                    ${tagTypes.map(([v, l]) => `<option value="${v}" ${state.extra.tag_type === v ? "selected" : ""}>${l}</option>`).join("")}
                </select>
                <select onchange="setParam('dir', this.value)">
                    <option value="">Newest</option>
                    <option value="asc" ${state.extra.dir === "asc" ? "selected" : ""}>Oldest</option>
                </select>
                <button onclick="doSearch()">Search</button>
            </div>
        </div>
        <table>
            <thead>
                <tr>
                    <th>UUID</th>
                    <th>Tags</th>
                    <th>Status</th>
                </tr>
            </thead>
            <tbody>
                ${players
                  .map(
                    (p) => `
                    <tr class="clickable" onclick="navigate('blacklist', '${p.uuid}')">
                        <td>${formatUuid(p.uuid)}</td>
                        <td>${p.tags.map((t) => renderTagBadge(t.tag_type)).join(" ") || '<span class="text-muted">-</span>'}</td>
                        <td>${p.is_locked ? '<span class="badge badge-locked">Locked</span>' : ""}</td>
                    </tr>
                `,
                  )
                  .join("")}
            </tbody>
        </table>
        ${renderPagination(total)}
    `;

  document.getElementById("search").addEventListener("keypress", (e) => {
    if (e.key === "Enter") doSearch();
  });
}

function renderSnapshotsList(main) {
  const { total, snapshots } = state.data;

  main.innerHTML = `
        <div class="header">
            <h2>Snapshots</h2>
            <div class="controls">
                <input type="text" id="search" placeholder="Search UUID or username..." value="${state.search}">
                <button onclick="doSearch()">Search</button>
            </div>
        </div>
        <table>
            <thead>
                <tr>
                    <th>Type</th>
                    <th>UUID</th>
                    <th>Username</th>
                    <th>Source</th>
                    <th>Timestamp</th>
                </tr>
            </thead>
            <tbody>
                ${snapshots
                  .map(
                    (s) => `
                    <tr class="clickable" onclick="navigate('snapshots', ${s.id})">
                        <td><span class="baseline-indicator ${s.is_baseline ? "is-baseline" : "is-delta"}"></span>${s.is_baseline ? "Baseline" : "Delta"}</td>
                        <td>${formatUuid(s.uuid)}</td>
                        <td>${s.username || '<span class="text-muted">-</span>'}</td>
                        <td>${s.source || "-"}</td>
                        <td>${formatDate(s.timestamp)}</td>
                    </tr>
                `,
                  )
                  .join("")}
            </tbody>
        </table>
        ${renderPagination(total)}
    `;

  document.getElementById("search").addEventListener("keypress", (e) => {
    if (e.key === "Enter") doSearch();
  });
}

function renderRateLimitsList(main) {
  const { total, rate_limits } = state.data;

  main.innerHTML = `
        <div class="header">
            <h2>Rate Limits</h2>
        </div>
        <table>
            <thead>
                <tr>
                    <th>API Key</th>
                    <th>Requests (window)</th>
                    <th>Created</th>
                </tr>
            </thead>
            <tbody>
                ${rate_limits
                  .map(
                    (r) => `
                    <tr>
                        <td><span class="mono">${r.api_key}...</span></td>
                        <td>${r.request_count || 0}</td>
                        <td>${formatDate(r.created_at)}</td>
                    </tr>
                `,
                  )
                  .join("")}
            </tbody>
        </table>
        <div class="pagination">
            <div class="pagination-info">Total: ${total}</div>
        </div>
    `;
}

function renderDiagnostics(main) {
  const { storage, players } = state.data;

  main.innerHTML = `
        <div class="header">
            <h2>Cache Diagnostics</h2>
        </div>

        <div class="detail-panel">
            <div class="detail-grid">
                <div class="detail-item">
                    <label>Total Snapshots</label>
                    <div class="value">${storage.total_snapshots.toLocaleString()}</div>
                </div>
                <div class="detail-item">
                    <label>Baselines</label>
                    <div class="value">${storage.total_baselines.toLocaleString()}</div>
                </div>
                <div class="detail-item">
                    <label>Deltas</label>
                    <div class="value">${storage.total_deltas.toLocaleString()}</div>
                </div>
                <div class="detail-item">
                    <label>Unique Players</label>
                    <div class="value">${storage.total_players.toLocaleString()}</div>
                </div>
                <div class="detail-item">
                    <label>Auto-Promotions</label>
                    <div class="value">${storage.total_promotions.toLocaleString()}</div>
                </div>
                <div class="detail-item">
                    <label>Avg Deltas/Baseline</label>
                    <div class="value">${storage.avg_deltas_per_baseline.toFixed(2)}</div>
                </div>
                <div class="detail-item">
                    <label>Storage Efficiency</label>
                    <div class="value">${storage.storage_efficiency.toFixed(1)}% deltas</div>
                </div>
            </div>
        </div>

        <h3 class="section-title">Top 50 Players by Delta Count (last 24h)</h3>
        <table>
            <thead>
                <tr>
                    <th>UUID</th>
                    <th>Username</th>
                    <th>Baselines</th>
                    <th>Deltas</th>
                    <th>Chain Length</th>
                    <th>Reconstruct Time</th>
                    <th>Baseline Age</th>
                </tr>
            </thead>
            <tbody>
                ${players
                  .map(
                    (p) => `
                    <tr>
                        <td>${formatUuid(p.uuid)}</td>
                        <td>${p.username || '<span class="text-muted">-</span>'}</td>
                        <td>${p.baseline_count}</td>
                        <td>${p.delta_count}</td>
                        <td>${p.delta_chain_length}</td>
                        <td>${formatReconstructTime(p.reconstruct_time_us)}</td>
                        <td>${formatBaselineAge(p.latest_baseline_age_hours)}</td>
                    </tr>
                `,
                  )
                  .join("")}
            </tbody>
        </table>
    `;
}

function formatReconstructTime(us) {
  if (us === null || us === undefined)
    return '<span class="text-muted">-</span>';
  if (us === 0) return '<span class="text-muted">0</span>';
  if (us < 1000) return `${us}µs`;
  if (us < 1000000) return `${(us / 1000).toFixed(2)}ms`;
  return `${(us / 1000000).toFixed(2)}s`;
}

function formatBaselineAge(hours) {
  if (hours === null || hours === undefined)
    return '<span class="text-muted">-</span>';
  if (hours < 1) return `${Math.round(hours * 60)}m ago`;
  if (hours < 24) return `${hours.toFixed(1)}h ago`;
  return `${(hours / 24).toFixed(1)}d ago`;
}

function renderDetail() {
  const main = document.getElementById("main");

  if (!state.data) {
    main.innerHTML = `
            <button class="back-btn" onclick="navigate('${state.page}')">← Back</button>
            <div class="empty">Not found</div>
        `;
    return;
  }

  switch (state.page) {
    case "members":
      renderMemberDetail(main);
      break;
    case "blacklist":
      renderBlacklistDetail(main);
      break;
  }
}

function renderMemberDetail(main) {
  const m = state.data;

  main.innerHTML = `
        <button class="back-btn" onclick="navigate('members')">← Back</button>
        <div class="detail-panel">
            <div class="detail-grid">
                <div class="detail-item">
                    <label>ID</label>
                    <div class="value">${m.id}</div>
                </div>
                <div class="detail-item">
                    <label>Discord ID</label>
                    <div class="value">${formatDiscordId(m.discord_id)}</div>
                </div>
                <div class="detail-item">
                    <label>UUID</label>
                    <div class="value">${formatUuid(m.uuid)}</div>
                </div>
                <div class="detail-item">
                    <label>API Key</label>
                    <div class="value"><span class="mono">${m.api_key_preview || "-"}${m.api_key_preview ? "..." : ""}</span></div>
                </div>
                <div class="detail-item">
                    <label>Access Level</label>
                    <div class="value">${renderBadges(m)}</div>
                </div>
                <div class="detail-item">
                    <label>Total Requests</label>
                    <div class="value">${m.request_count.toLocaleString()}</div>
                </div>
                <div class="detail-item">
                    <label>Joined</label>
                    <div class="value">${formatDate(m.join_date)}</div>
                </div>
                <div class="detail-item">
                    <label>Updated</label>
                    <div class="value">${formatDate(m.updated_at)}</div>
                </div>
            </div>
        </div>

        ${
          m.ips.length > 0
            ? `
            <h3 class="section-title">IP History (${m.ips.length})</h3>
            <table>
                <thead>
                    <tr>
                        <th>IP Address</th>
                        <th>First Seen</th>
                        <th>Last Seen</th>
                    </tr>
                </thead>
                <tbody>
                    ${m.ips
                      .map(
                        (ip) => `
                        <tr>
                            <td><span class="mono">${ip.ip_address}</span></td>
                            <td>${formatDate(ip.first_seen)}</td>
                            <td>${formatDate(ip.last_seen)}</td>
                        </tr>
                    `,
                      )
                      .join("")}
                </tbody>
            </table>
        `
            : ""
        }

        ${
          m.alt_accounts.length > 0
            ? `
            <h3 class="section-title">Alt Accounts (${m.alt_accounts.length})</h3>
            <table>
                <thead>
                    <tr>
                        <th>UUID</th>
                        <th>Added</th>
                    </tr>
                </thead>
                <tbody>
                    ${m.alt_accounts
                      .map(
                        (a) => `
                        <tr>
                            <td>${formatUuid(a.uuid)}</td>
                            <td>${formatDate(a.added_at)}</td>
                        </tr>
                    `,
                      )
                      .join("")}
                </tbody>
            </table>
        `
            : ""
        }

        <h3 class="section-title">Config</h3>
        <div class="json-viewer">${JSON.stringify(m.config, null, 2)}</div>
    `;
}

function renderBlacklistDetail(main) {
  const { player, tags, tag_history } = state.data;

  main.innerHTML = `
        <button class="back-btn" onclick="navigate('blacklist')">← Back</button>
        <div class="detail-panel">
            <div class="detail-grid">
                <div class="detail-item">
                    <label>UUID</label>
                    <div class="value">${formatUuid(player.uuid)}</div>
                </div>
                <div class="detail-item">
                    <label>Status</label>
                    <div class="value">${player.is_locked ? '<span class="badge badge-locked">Locked</span>' : '<span class="text-muted">Active</span>'}</div>
                </div>
                ${
                  player.lock_reason
                    ? `
                    <div class="detail-item">
                        <label>Lock Reason</label>
                        <div class="value">${player.lock_reason}</div>
                    </div>
                `
                    : ""
                }
            </div>
        </div>

        <h3 class="section-title">Active Tags (${tags.length})</h3>
        ${
          tags.length > 0
            ? `
            <div class="tags-list">
                ${tags
                  .map(
                    (t) => `
                    <div class="tag-card">
                        <div class="tag-type">${renderTagBadge(t.tag_type)}</div>
                        <div class="tag-reason">${t.reason}</div>
                        <div class="tag-meta">
                            Added by ${t.added_by} on ${formatDate(t.added_on)}
                            ${t.hide_username ? " • Username hidden" : ""}
                        </div>
                    </div>
                `,
                  )
                  .join("")}
            </div>
        `
            : '<div class="empty">No active tags</div>'
        }

        ${
          tag_history.length > 0
            ? `
            <h3 class="section-title">Tag History (${tag_history.length} removed)</h3>
            <div class="tags-list">
                ${tag_history
                  .map(
                    (t) => `
                    <div class="tag-card" style="opacity: 0.6">
                        <div class="tag-type">${renderTagBadge(t.tag_type)}</div>
                        <div class="tag-reason">${t.reason}</div>
                        <div class="tag-meta">
                            Added by ${t.added_by} on ${formatDate(t.added_on)}<br>
                            Removed by ${t.removed_by} on ${formatDate(t.removed_on)}
                        </div>
                    </div>
                `,
                  )
                  .join("")}
            </div>
        `
            : ""
        }
    `;
}

function renderSnapshotDetail(main) {
  const s = state.data;

  main.innerHTML = `
        <button class="back-btn" onclick="navigate('snapshots')">← Back</button>
        <div class="detail-panel">
            <div class="detail-grid">
                <div class="detail-item">
                    <label>Type</label>
                    <div class="value"><span class="baseline-indicator ${s.is_baseline ? "is-baseline" : "is-delta"}"></span>${s.is_baseline ? "Baseline" : "Delta"}</div>
                </div>
                <div class="detail-item">
                    <label>UUID</label>
                    <div class="value">${formatUuid(s.uuid)}</div>
                </div>
                <div class="detail-item">
                    <label>Username</label>
                    <div class="value">${s.username || "-"}</div>
                </div>
                <div class="detail-item">
                    <label>Source</label>
                    <div class="value">${s.source || "-"}</div>
                </div>
                <div class="detail-item">
                    <label>Discord ID</label>
                    <div class="value">${s.discord_id ? formatDiscordId(s.discord_id) : "-"}</div>
                </div>
                <div class="detail-item">
                    <label>Timestamp</label>
                    <div class="value">${formatDate(s.timestamp)}</div>
                </div>
            </div>
        </div>

        <h3 class="section-title">Data</h3>
        <div class="json-viewer">${JSON.stringify(s.data, null, 2)}</div>
    `;
}

function esc(s) {
  return String(s).replace(
    /[&<>]/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c],
  );
}

function renderPlaceholder(title, desc) {
  document.getElementById("main").innerHTML =
    `<div class="header"><h2>${title}</h2></div><div class="empty">Coming soon — ${desc}.</div>`;
}

function renderPlayersList(main) {
  const players = state.data.players || [];
  main.innerHTML = `
        <div class="header">
            <h2>Players</h2>
            <div class="controls">
                <input type="text" id="search" placeholder="UUID, username, or linked Discord..." value="${state.search}">
                <button onclick="doSearch()">Search</button>
            </div>
        </div>
        <table>
            <thead><tr><th>UUID</th><th>Username</th><th>Last Snapshot</th></tr></thead>
            <tbody>
                ${players
                  .map(
                    (p) => `
                    <tr class="clickable" onclick="navigate('players', '${p.uuid}')">
                        <td>${formatUuid(p.uuid)}</td>
                        <td>${p.username ? esc(p.username) : '<span class="text-muted">-</span>'}</td>
                        <td>${formatDate(p.last_snapshot_at)}</td>
                    </tr>
                `,
                  )
                  .join("")}
            </tbody>
        </table>
        ${
          state.search
            ? `<div class="pagination"><div class="pagination-info">${players.length} match${players.length === 1 ? "" : "es"}</div></div>`
            : renderCursorPagination(players.length)
        }
    `;
  document.getElementById("search").addEventListener("keypress", (e) => {
    if (e.key === "Enter") doSearch();
  });
}

function renderGuildsList(main) {
  const { total, guilds } = state.data;
  main.innerHTML = `
        <div class="header">
            <h2>Guilds</h2>
            <div class="controls">
                <input type="text" id="search" placeholder="Name, tag, or guild ID..." value="${state.search}">
                <select onchange="setParam('sort', this.value)">
                    <option value="">Recently updated</option>
                    <option value="members" ${state.extra.sort === "members" ? "selected" : ""}>Members</option>
                    <option value="level" ${state.extra.sort === "level" ? "selected" : ""}>Level</option>
                    <option value="experience" ${state.extra.sort === "experience" ? "selected" : ""}>Experience</option>
                </select>
                <button onclick="doSearch()">Search</button>
            </div>
        </div>
        <table>
            <thead><tr><th>Name</th><th>Tag</th><th>Members</th><th>Level</th><th>Updated</th></tr></thead>
            <tbody>
                ${guilds
                  .map(
                    (g) => `
                    <tr class="clickable" onclick="navigate('guilds', '${g.guild_id}')">
                        <td>${esc(g.name)}</td>
                        <td>${g.tag ? esc(g.tag) : '<span class="text-muted">-</span>'}</td>
                        <td>${g.member_count}</td>
                        <td>${g.level}</td>
                        <td>${formatDate(g.updated_at)}</td>
                    </tr>
                `,
                  )
                  .join("")}
            </tbody>
        </table>
        ${renderPagination(total)}
    `;
  document.getElementById("search").addEventListener("keypress", (e) => {
    if (e.key === "Enter") doSearch();
  });
}

async function loadGuildView() {
  state.data = await api(`/guilds/${state.detail}`);
  state.guildTs = null;
  renderGuildView(state.data.current);
}

function renderGuildView(snapshot) {
  const view = state.data;
  const stamps = view.timestamps || [];
  document.getElementById("main").innerHTML = `
        <button class="back-btn" onclick="navigate('guilds')">← Back</button>
        <div class="header">
            <h2>${view.name ? esc(view.name) : "(guild)"} <span class="mono text-muted" style="font-size:0.55em">${view.guild_id}</span></h2>
            <div class="controls">
                <input type="datetime-local" id="atTime">
                <button onclick="jumpGuildTime()">Go</button>
                <button onclick="showGuildCurrent()">Current</button>
            </div>
        </div>
        <div style="display:flex; gap:16px; align-items:flex-start">
            <div style="flex:0 0 220px; max-height:72vh; overflow:auto">
                <h3 class="section-title">Snapshots (${stamps.length})</h3>
                ${stamps
                  .map(
                    (ts) =>
                      `<div class="clickable" style="padding:4px 8px;${ts === state.guildTs ? "background:rgba(255,255,255,0.08)" : ""}" onclick="loadGuildAt('${ts}')">${formatDate(ts)}</div>`,
                  )
                  .join("")}
            </div>
            <div style="flex:1; min-width:0">
                <h3 class="section-title">${state.guildTs ? formatDate(state.guildTs) : "Current (reconstructed)"}</h3>
                <div class="json-viewer">${snapshot ? esc(JSON.stringify(snapshot, null, 2)) : "No snapshot"}</div>
            </div>
        </div>
    `;
}

async function loadGuildAt(ts) {
  state.guildTs = ts;
  renderGuildView(await api(`/guilds/${state.detail}/at?ts=${encodeURIComponent(ts)}`));
}

function showGuildCurrent() {
  state.guildTs = null;
  renderGuildView(state.data.current);
}

async function jumpGuildTime() {
  const v = document.getElementById("atTime").value;
  if (!v) return;
  const ms = new Date(v).getTime();
  state.guildTs = new Date(ms).toISOString();
  renderGuildView(await api(`/guilds/${state.detail}/at?ts=${ms}`));
}

const charts = {};
const CW = 1000;
const PAD = { l: 52, r: 16, t: 14, b: 26 };

function niceMax(v) {
  if (v <= 0) return 1;
  const pow = Math.pow(10, Math.floor(Math.log10(v)));
  const f = v / pow;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * pow;
}

function fmtNum(v) {
  return Math.round(v).toLocaleString();
}

function fmtMs(v) {
  if (v === null || v === undefined) return "—";
  if (v <= 0) return "0";
  if (v < 1) return "<1 ms";
  if (v < 1000) return `${Math.round(v)} ms`;
  return `${(v / 1000).toFixed(2)} s`;
}

function fmtTick(iso, hours) {
  const d = new Date(iso);
  return hours <= 24
    ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleString([], { month: "short", day: "numeric", hour: "2-digit" });
}

function gridY(yMax, ph, fmt) {
  const out = [];
  for (let i = 0; i <= 4; i++) {
    const y = PAD.t + ph * (1 - i / 4);
    out.push(
      `<line class="grid" x1="${PAD.l}" y1="${y}" x2="${CW - PAD.r}" y2="${y}"/>` +
        `<text class="axlbl" x="${PAD.l - 8}" y="${y + 3.5}" text-anchor="end">${fmt((yMax * i) / 4)}</text>`,
    );
  }
  return out.join("");
}

function axisXpts(points, xs, ph, xFmt) {
  const n = points.length;
  const want = Math.min(7, n);
  const y = PAD.t + ph + 17;
  const out = [];
  for (let k = 0; k < want; k++) {
    const i = want > 1 ? Math.round((k * (n - 1)) / (want - 1)) : 0;
    out.push(
      `<text class="axlbl" x="${xs[i].toFixed(1)}" y="${y}" text-anchor="middle">${xFmt(points[i].t)}</text>`,
    );
  }
  return out.join("");
}

function crosshair(ph, n) {
  return (
    `<line class="cross" x1="0" x2="0" y1="${PAD.t}" y2="${PAD.t + ph}"/>` +
    Array.from({ length: n }, () => `<circle class="cdot" r="3.5"/>`).join("")
  );
}

function chartSvg(id, points, series, opts) {
  if (!points || !points.length) return "";
  const H = opts.height || 240;
  const ph = H - PAD.t - PAD.b;
  const n = points.length;
  const plotW = CW - PAD.l - PAD.r;
  const xs = points.map((_, i) =>
    n > 1 ? PAD.l + (i / (n - 1)) * plotW : PAD.l + plotW / 2,
  );
  const yMax =
    opts.yMax != null
      ? opts.yMax
      : niceMax(
          Math.max(1, ...series.flatMap((s) => points.map((p) => +p[s.key] || 0))),
        );
  const y = (v) => PAD.t + ph * (1 - Math.min(v, yMax) / yMax);
  const line = (key) =>
    points
      .map((p, i) => `${i ? "L" : "M"}${xs[i].toFixed(1)} ${y(+p[key] || 0).toFixed(1)}`)
      .join(" ");
  const area = (key) =>
    `${line(key)} L${xs[n - 1].toFixed(1)} ${(PAD.t + ph).toFixed(1)} L${xs[0].toFixed(1)} ${(PAD.t + ph).toFixed(1)} Z`;
  const body = series
    .map(
      (s) =>
        (s.fill ? `<path d="${area(s.key)}" fill="${s.fill}"/>` : "") +
        `<path d="${line(s.key)}" fill="none" stroke="${s.stroke}" stroke-width="1.5" ${s.dashed ? 'stroke-dasharray="5 4"' : ""} stroke-linejoin="round" vector-effect="non-scaling-stroke"/>`,
    )
    .join("");
  charts[id] = {
    points,
    n,
    xs,
    step: n > 1 ? plotW / (n - 1) : plotW,
    ys: series.map((s) => points.map((p) => y(+p[s.key] || 0))),
    dotColors: series.map((s) => s.stroke),
    tip: opts.tip,
    onClick: opts.onClick,
  };
  return (
    `<svg id="${id}" viewBox="0 0 ${CW} ${H}" preserveAspectRatio="none" class="chart-svg${opts.onClick ? " clickable-chart" : ""}" style="aspect-ratio:${CW}/${H}">` +
    gridY(yMax, ph, opts.yFmt) +
    body +
    axisXpts(points, xs, ph, opts.xFmt) +
    crosshair(ph, series.length) +
    `</svg><div class="chart-tip"></div>`
  );
}

function renderChart() {
  const el = document.getElementById("api-chart");
  const lg = document.getElementById("chart-legend");
  if (!el) return;
  const hyp = apiState.mode === "hypixel";
  const noun = hyp ? "outgoing" : "requests";
  const pts = apiState.series || [];
  lg.innerHTML = `<span class="lg"><i style="background:rgba(255,255,255,0.6)"></i>${hyp ? "Outgoing" : "Requests"}</span><span class="lg"><i style="background:var(--danger)"></i>Errors</span>`;
  if (!pts.length) {
    el.innerHTML = `<div class="chart-empty">${hyp ? "No outgoing Hypixel requests recorded for this window yet" : "No traffic in this window"}</div>`;
    return;
  }
  el.innerHTML = chartSvg(
    "cht",
    pts,
    [
      { key: "total", stroke: "rgba(255,255,255,0.65)", fill: "rgba(255,255,255,0.06)" },
      { key: "errors", stroke: "var(--danger)", fill: "rgba(248,113,113,0.13)" },
    ],
    {
      yFmt: fmtNum,
      xFmt: (t) => fmtTick(t, apiState.hours),
      tip: (p) =>
        `<b>${fmtTick(p.t, apiState.hours)}</b><span>${fmtNum(p.total)} ${noun}</span>${p.errors ? `<span class="tip-err">${fmtNum(p.errors)} errors</span>` : ""}${hyp ? "" : '<span class="tip-hint">click to inspect</span>'}`,
      onClick: hyp ? null : selectSlot,
    },
  );
  mountChart("cht");
}

function mountChart(id) {
  const c = charts[id];
  const svg = document.getElementById(id);
  if (!c || !svg) return;
  const tip = svg.parentElement.querySelector(".chart-tip");
  const cross = svg.querySelector(".cross");
  const dots = [...svg.querySelectorAll(".cdot")];
  dots.forEach((d, i) =>
    d.setAttribute("fill", c.dotColors[i] || "rgba(255,255,255,0.7)"),
  );

  svg.addEventListener("mousemove", (e) => {
    const r = svg.getBoundingClientRect();
    const mx = ((e.clientX - r.left) / r.width) * CW;
    const i = c.n > 1 ? Math.max(0, Math.min(c.n - 1, Math.round((mx - PAD.l) / c.step))) : 0;
    const px = c.xs[i];
    cross.setAttribute("x1", px);
    cross.setAttribute("x2", px);
    cross.classList.add("on");
    c.ys.forEach((ys, si) => {
      dots[si].setAttribute("cx", px);
      dots[si].setAttribute("cy", ys[i]);
      dots[si].classList.add("on");
    });
    tip.innerHTML = c.tip(c.points[i]);
    tip.style.left = `${svg.offsetLeft + (px / CW) * svg.offsetWidth}px`;
    tip.classList.add("on");
  });
  svg.addEventListener("mouseleave", () => {
    cross.classList.remove("on");
    dots.forEach((d) => d.classList.remove("on"));
    tip.classList.remove("on");
  });
  if (c.onClick) {
    svg.addEventListener("click", (e) => {
      const r = svg.getBoundingClientRect();
      const mx = ((e.clientX - r.left) / r.width) * CW;
      const i = c.n > 1 ? Math.max(0, Math.min(c.n - 1, Math.round((mx - PAD.l) / c.step))) : 0;
      c.onClick(c.points[i]);
    });
  }
}

const apiState = {
  hours: 24,
  mode: "incoming",
  path: "",
  paths: [],
  series: [],
  filter: {},
  offset: 0,
  limit: 50,
  paused: false,
};

const STATUS_CLASSES = {
  1: ["1xx", "rgba(255,255,255,0.4)"],
  2: ["2xx", "var(--ok)"],
  3: ["3xx", "rgba(255,255,255,0.55)"],
  4: ["4xx", "var(--warning)"],
  5: ["5xx", "var(--danger)"],
};

function statusBar(classes, total) {
  if (!total) return "";
  const seg = classes
    .map((c) => {
      const [label, color] = STATUS_CLASSES[c.class] || [`${c.class}xx`, "rgba(255,255,255,0.4)"];
      const pct = (c.count / total) * 100;
      return `<div class="sb-seg" style="width:${pct}%;background:${color}" title="${label}: ${fmtNum(c.count)} (${pct.toFixed(1)}%)"></div>`;
    })
    .join("");
  const legend = classes
    .map((c) => {
      const [label, color] = STATUS_CLASSES[c.class] || [`${c.class}xx`, "rgba(255,255,255,0.4)"];
      return `<span class="lg"><i style="background:${color}"></i>${label} ${fmtNum(c.count)}</span>`;
    })
    .join("");
  return `<div class="status-row"><div class="status-bar">${seg}</div><div class="legend">${legend}</div></div>`;
}

function card(label, value, sub, cls = "") {
  return `<div class="card"><div class="card-label">${label}</div><div class="card-value ${cls}">${value}</div>${sub ? `<div class="card-sub">${sub}</div>` : ""}</div>`;
}

function val(r, def) {
  return r.status === "fulfilled" && r.value ? r.value : def;
}

function logQuery() {
  const p = new URLSearchParams();
  p.set("hours", apiState.hours);
  p.set("limit", apiState.limit);
  p.set("offset", apiState.offset);
  for (const [k, v] of Object.entries(apiState.filter)) if (v) p.set(k, v);
  return p.toString();
}

async function renderApiDashboard() {
  clearTimeout(state.refreshTimer);
  buildApiShell();
  apiState.paths = await api(`/requests/paths?hours=${apiState.hours}`).catch(() => []);
  populateEndpoints();
  await refreshApi();
  startApiLoop();
}

function buildApiShell() {
  const windows = [
    ["1", "1h"],
    ["6", "6h"],
    ["24", "24h"],
    ["72", "3d"],
    ["168", "7d"],
    ["336", "14d"],
  ];
  const seg = (id, items, active, fn, attr) =>
    `<div class="seg" id="${id}">${items
      .map(
        ([v, l]) =>
          `<button ${attr}="${v}" class="${active == v ? "active" : ""}" onclick="${fn}('${v}')">${l}</button>`,
      )
      .join("")}</div>`;

  document.getElementById("main").innerHTML = `
    <div class="header">
      <h2>API</h2>
      ${seg("win-seg", windows, apiState.hours, "setHours", "data-h")}
    </div>
    <div id="api-cards"></div>
    <div class="panel chart-panel" id="chart-panel">
      <div class="chart-toolbar">
        ${seg("mode-seg", [["incoming", "Incoming"], ["endpoint", "Endpoint"], ["hypixel", "Hypixel"]], apiState.mode, "setMode", "data-mode")}
        <select id="ep-select" class="ep-select" style="display:${apiState.mode === "endpoint" ? "" : "none"}" onchange="setPath(this.value)"></select>
        <span class="chart-legend" id="chart-legend"></span>
      </div>
      <div id="api-chart" class="chart-body"></div>
    </div>
    <div class="two-col">
      <div class="panel"><div class="panel-title">Top callers</div><div id="api-keys"></div></div>
      <div class="panel"><div class="panel-title">Top endpoints</div><div id="api-paths"></div></div>
    </div>
    <div class="panel" id="log-panel">
      <div class="panel-title">Recent requests</div>
      <div class="log-filters">
        <input id="flt_path" placeholder="path contains…" value="${apiState.filter.path || ""}">
        <input id="flt_status" placeholder="status" value="${apiState.filter.status || ""}" style="width:90px">
        <input id="flt_key" placeholder="key" value="${apiState.filter.key_prefix || ""}" style="width:120px">
        <input id="flt_ip" placeholder="ip" value="${apiState.filter.ip || ""}" style="width:150px">
        <label class="check"><input type="checkbox" id="flt_err" ${apiState.filter.errors ? "checked" : ""} onchange="toggleErr(this.checked)"> errors only</label>
        <button onclick="applyLogFilters()">Filter</button>
      </div>
      <div id="api-log"></div>
    </div>`;

  ["flt_path", "flt_status", "flt_key", "flt_ip"].forEach((id) =>
    document.getElementById(id).addEventListener("keydown", (e) => {
      if (e.key === "Enter") applyLogFilters();
    }),
  );
  ["chart-panel", "log-panel"].forEach((id) => {
    const el = document.getElementById(id);
    el.addEventListener("mouseenter", () => (apiState.paused = true));
    el.addEventListener("mouseleave", () => (apiState.paused = false));
  });
}

async function refreshApi() {
  const h = apiState.hours;
  const seriesUrl =
    apiState.mode === "hypixel"
      ? `/requests/hypixel-series?hours=${h}`
      : apiState.mode === "endpoint" && apiState.path
        ? `/requests/series?hours=${h}&path=${encodeURIComponent(apiState.path)}`
        : `/requests/series?hours=${h}`;
  const res = await Promise.allSettled([
    api(`/requests/stats?hours=${h}`),
    api(`/requests/ratelimits`),
    api(`/requests?${logQuery()}`),
    api(seriesUrl),
  ]);
  if (!document.getElementById("api-cards")) return;
  const stats = val(res[0], { total: 0, errors: 0, status_classes: [], top_keys: [], top_paths: [] });
  const rl = val(res[1], { available: false, capacity: 0, used: 0, headroom: 0 });
  const log = val(res[2], { total: 0, requests: [] });
  apiState.series = val(res[3], []);
  patchCards(stats, rl);
  patchKeys(stats.top_keys);
  patchPaths(stats.top_paths);
  patchLog(log);
  if (!apiState.paused) renderChart();
  resolveNames();
}

function patchCards(stats, rl) {
  const el = document.getElementById("api-cards");
  if (!el) return;
  const errRate = stats.total ? (stats.errors / stats.total) * 100 : 0;
  const rps = stats.total / (apiState.hours * 3600);
  const pct = rl.capacity ? (rl.used / rl.capacity) * 100 : 0;
  const capColor = pct > 85 ? "var(--danger)" : pct > 60 ? "var(--warning)" : "var(--ok)";
  el.innerHTML = `
    <div class="cards">
      ${card("Requests", fmtNum(stats.total), `${rps < 1 ? rps.toFixed(2) : Math.round(rps)}/s · ${apiState.hours}h`)}
      ${card("Errors", `${errRate.toFixed(1)}%`, `${fmtNum(stats.errors)} total`, errRate > 5 ? "danger" : "")}
      ${card("Avg latency", fmtMs(stats.avg_ms), "response time")}
      <div class="card">
        <div class="card-label">Hypixel headroom</div>
        <div class="card-value">${rl.available ? fmtNum(rl.headroom) : "—"}</div>
        ${
          rl.available
            ? `<div class="meter"><div class="meter-fill" style="width:${Math.min(100, pct).toFixed(1)}%;background:${capColor}"></div></div><div class="card-sub">${fmtNum(rl.used)} / ${fmtNum(rl.capacity)} used</div>`
            : '<div class="card-sub">redis offline</div>'
        }
      </div>
    </div>
    ${statusBar(stats.status_classes || [], stats.total)}`;
}

function patchKeys(keys) {
  const el = document.getElementById("api-keys");
  if (!el) return;
  el.innerHTML =
    keys && keys.length
      ? `<table><thead><tr><th>Key</th><th>Account</th><th>Discord</th><th>Reqs</th><th>Err</th></tr></thead><tbody>${keys
          .map(
            (k) => `<tr class="clickable" onclick="filterByKey('${k.key_prefix ? esc(k.key_prefix) : ""}')">
            <td><span class="mono">${k.key_prefix ? esc(k.key_prefix) : "none"}</span></td>
            <td>${k.uuid ? formatUuid(k.uuid) : '<span class="text-muted">—</span>'}</td>
            <td>${k.discord_id ? formatDiscordId(k.discord_id) : '<span class="text-muted">—</span>'}</td>
            <td>${fmtNum(k.count)}</td>
            <td>${k.errors ? `<span class="text-danger">${fmtNum(k.errors)}</span>` : '<span class="text-muted">—</span>'}</td>
          </tr>`,
          )
          .join("")}</tbody></table>`
      : '<div class="muted-row">No data</div>';
}

function patchPaths(paths) {
  const el = document.getElementById("api-paths");
  if (!el) return;
  el.innerHTML =
    paths && paths.length
      ? `<table><thead><tr><th>Path</th><th>Reqs</th><th>Err</th><th>Avg</th></tr></thead><tbody>${paths
          .map(
            (p) => `<tr class="clickable" onclick="selectEndpoint('${p.path ? esc(p.path) : ""}')">
            <td class="path-cell"><span class="mono">${p.path ? esc(p.path) : "—"}</span></td>
            <td>${fmtNum(p.count)}</td>
            <td>${p.errors ? `<span class="text-danger">${fmtNum(p.errors)}</span>` : '<span class="text-muted">—</span>'}</td>
            <td>${fmtMs(p.avg_ms)}</td>
          </tr>`,
          )
          .join("")}</tbody></table>`
      : '<div class="muted-row">No data</div>';
}

function patchLog(log) {
  const el = document.getElementById("api-log");
  if (!el) return;
  apiState.log = log;
  const chips = [];
  if (apiState.filter.from && apiState.filter.to) {
    const f = new Date(apiState.filter.from * 1000).toLocaleTimeString();
    const t = new Date(apiState.filter.to * 1000).toLocaleTimeString();
    chips.push(`<span class="chip">slot: ${f}–${t} <a onclick="clearFilter('time')">✕</a></span>`);
  }
  for (const [k, v] of Object.entries(apiState.filter)) {
    if (!v || k === "from" || k === "to") continue;
    chips.push(
      `<span class="chip">${k === "key_prefix" ? "key" : k}: ${esc(String(v))} <a onclick="clearFilter('${k}')">✕</a></span>`,
    );
  }
  const caller = (r) =>
    r.uuid
      ? formatUuid(r.uuid)
      : r.discord_id
        ? formatDiscordId(r.discord_id)
        : "";
  const rows = log.requests.length
    ? log.requests
        .map(
          (r, i) => `<tr class="clickable ${r.status >= 400 ? "row-err" : ""}" onclick="showRequest(${i})">
        <td class="nowrap text-muted">${formatDate(r.ts)}</td>
        <td><span class="method m-${(r.method || "").toLowerCase()}">${r.method || "—"}</span></td>
        <td class="path-cell"><span class="mono">${r.path ? esc(r.path) : "—"}</span></td>
        <td><span class="status-chip s-${Math.floor((r.status || 0) / 100)}">${r.status ?? "—"}</span></td>
        <td class="text-muted">${r.latency_ms == null ? "—" : fmtMs(r.latency_ms)}</td>
        <td>${r.key_prefix ? `<a class="mono link" onclick="event.stopPropagation();filterByKey('${esc(r.key_prefix)}')">${esc(r.key_prefix)}</a>` : '<span class="text-muted">—</span>'}${caller(r) ? ` <span class="caller">${caller(r)}</span>` : ""}</td>
        <td><span class="mono">${r.ip ? esc(r.ip) : "—"}</span></td>
      </tr>`,
        )
        .join("")
    : '<tr><td colspan="7" class="muted-row">No requests match</td></tr>';
  el.innerHTML = `
    ${chips.length ? `<div class="chips">${chips.join("")}</div>` : ""}
    <table class="log-table">
      <thead><tr><th>Time</th><th>Method</th><th>Path</th><th>Status</th><th>Latency</th><th>Key</th><th>IP</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    ${logPagination(log.total)}`;
}

function prettyJson(s) {
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch {
    return s;
  }
}

function showRequest(i) {
  const r = apiState.log?.requests?.[i];
  if (!r) return;
  const url = (r.path || "") + (r.query ? "?" + r.query : "");
  const field = (label, value) => `<div class="mf"><label>${label}</label><div>${value}</div></div>`;
  const fields = [
    field("Time", formatDate(r.ts)),
    field("Status", `<span class="status-chip s-${Math.floor((r.status || 0) / 100)}">${r.status ?? "—"}</span>`),
    field("Latency", r.latency_ms == null ? "—" : fmtMs(r.latency_ms)),
    field("Key", r.key_prefix ? `<span class="mono">${esc(r.key_prefix)}</span>` : "—"),
    field("Account", r.uuid ? formatUuid(r.uuid) : "—"),
    field("Discord", r.discord_id ? formatDiscordId(r.discord_id) : "—"),
    field("IP", r.ip ? `<span class="mono">${esc(r.ip)}</span>` : "—"),
    field("User-Agent", r.user_agent ? esc(r.user_agent) : "—"),
  ].join("");
  const errorBlock = r.error
    ? `<div class="modal-section"><label>Error response</label><pre class="json-viewer">${esc(prettyJson(r.error))}</pre></div>`
    : r.status >= 400
      ? '<div class="modal-section text-muted">No response body captured (predates logging, or the body was empty).</div>'
      : "";
  document.getElementById("modal-body").innerHTML = `
    <div class="modal-head">
      <div><span class="method m-${(r.method || "").toLowerCase()}">${r.method || "—"}</span> <span class="mono modal-url">${esc(url)}</span></div>
      <button class="modal-close" onclick="closeModal()">✕</button>
    </div>
    <div class="modal-grid">${fields}</div>
    ${errorBlock}`;
  document.getElementById("modal").classList.add("open");
  resolveNames();
}

function closeModal(e) {
  if (e && e.currentTarget !== e.target) return;
  document.getElementById("modal").classList.remove("open");
}

function logPagination(total) {
  const start = apiState.offset + 1;
  const end = Math.min(apiState.offset + apiState.limit, total);
  const prev = apiState.offset > 0;
  const next = apiState.offset + apiState.limit < total;
  return `<div class="pagination">
      <div class="pagination-info">${total ? `${start}–${end} of ${fmtNum(total)}` : "0 requests"}</div>
      <div class="pagination-buttons">
        <button ${!prev ? "disabled" : ""} onclick="logPage(-1)">Prev</button>
        <button ${!next ? "disabled" : ""} onclick="logPage(1)">Next</button>
      </div>
    </div>`;
}

function populateEndpoints() {
  const sel = document.getElementById("ep-select");
  if (!sel) return;
  if (!apiState.path && apiState.paths[0]) apiState.path = apiState.paths[0].path;
  sel.innerHTML = apiState.paths
    .map(
      (p) =>
        `<option value="${esc(p.path || "")}" ${p.path === apiState.path ? "selected" : ""}>${esc(p.path || "(none)")} · ${fmtNum(p.count)}</option>`,
    )
    .join("");
}

function markActive(segId, attr, v) {
  document
    .querySelectorAll(`#${segId} button`)
    .forEach((b) => b.classList.toggle("active", b.getAttribute(attr) == v));
}

function startApiLoop() {
  clearTimeout(state.refreshTimer);
  state.refreshTimer = setTimeout(async () => {
    if (state.page !== "api" || state.detail) return;
    const focused = ["INPUT", "SELECT"].includes(document.activeElement?.tagName);
    if (!apiState.paused && !focused) await refreshApi();
    startApiLoop();
  }, 5000);
}

function setHours(h) {
  apiState.hours = +h;
  apiState.offset = 0;
  markActive("win-seg", "data-h", h);
  api(`/requests/paths?hours=${h}`)
    .then((p) => {
      apiState.paths = p;
      populateEndpoints();
    })
    .catch(() => {});
  refreshApi();
}

function setMode(m) {
  apiState.mode = m;
  markActive("mode-seg", "data-mode", m);
  document.getElementById("ep-select").style.display = m === "endpoint" ? "" : "none";
  refreshApi();
}

function setPath(p) {
  apiState.path = p;
  refreshApi();
}

function selectEndpoint(p) {
  apiState.mode = "endpoint";
  apiState.path = p;
  markActive("mode-seg", "data-mode", "endpoint");
  const sel = document.getElementById("ep-select");
  sel.style.display = "";
  populateEndpoints();
  refreshApi();
}

function filterByKey(kp) {
  if (!kp) return;
  apiState.filter.key_prefix = kp;
  apiState.offset = 0;
  refreshApi();
  document.getElementById("log-panel")?.scrollIntoView({ behavior: "smooth" });
}

function clearFilter(k) {
  if (k === "time") {
    delete apiState.filter.from;
    delete apiState.filter.to;
  } else {
    delete apiState.filter[k];
  }
  apiState.offset = 0;
  const input = { path: "flt_path", status: "flt_status", key_prefix: "flt_key", ip: "flt_ip" }[k];
  if (input) document.getElementById(input).value = "";
  if (k === "errors") document.getElementById("flt_err").checked = false;
  refreshApi();
}

function selectSlot(point) {
  const pts = apiState.series || [];
  const i = pts.indexOf(point);
  const startMs = new Date(point.t).getTime();
  let width =
    pts.length > 1
      ? Math.abs(new Date(pts[1].t) - new Date(pts[0].t))
      : apiState.hours * 3600 * 1000;
  if (i >= 0 && i < pts.length - 1) width = new Date(pts[i + 1].t) - new Date(point.t);
  apiState.filter.from = Math.floor(startMs / 1000);
  apiState.filter.to = Math.floor(startMs / 1000) + Math.round(width / 1000);
  apiState.offset = 0;
  refreshApi();
  document.getElementById("log-panel")?.scrollIntoView({ behavior: "smooth" });
}

function toggleErr(on) {
  if (on) apiState.filter.errors = "true";
  else delete apiState.filter.errors;
  apiState.offset = 0;
  refreshApi();
}

function logPage(d) {
  apiState.offset = Math.max(0, apiState.offset + d * apiState.limit);
  refreshApi();
}

function applyLogFilters() {
  for (const [k, id] of [
    ["path", "flt_path"],
    ["status", "flt_status"],
    ["key_prefix", "flt_key"],
    ["ip", "flt_ip"],
  ]) {
    const v = document.getElementById(id).value.trim();
    if (v) apiState.filter[k] = v;
    else delete apiState.filter[k];
  }
  apiState.offset = 0;
  refreshApi();
}

function renderCursorPagination(count) {
  const hasPrev = state.offset > 0;
  const hasNext = count === state.limit;
  return `
        <div class="pagination">
            <div class="pagination-info">Showing ${state.offset + 1}-${state.offset + count}</div>
            <div class="pagination-buttons">
                <button ${!hasPrev ? "disabled" : ""} onclick="setOffset(${state.offset - state.limit})">Previous</button>
                <button ${!hasNext ? "disabled" : ""} onclick="setOffset(${state.offset + state.limit})">Next</button>
            </div>
        </div>
    `;
}

async function loadPlayerView() {
  state.data = await api(`/players/${state.detail}`);
  state.playerTs = null;
  renderPlayerView(state.data.latest);
}

function renderPlayerView(snapshot) {
  const view = state.data;
  const stamps = view.timestamps || [];
  document.getElementById("main").innerHTML = `
        <button class="back-btn" onclick="navigate('players')">← Back</button>
        <div class="header">
            <h2>${view.username ? esc(view.username) : "(unknown)"} <span class="mono text-muted" style="font-size:0.55em">${view.uuid}</span></h2>
            <div class="controls">
                <input type="datetime-local" id="atTime">
                <button onclick="jumpToTime()">Go</button>
                <button onclick="showLatest()">Latest</button>
            </div>
        </div>
        <div style="display:flex; gap:16px; align-items:flex-start">
            <div style="flex:0 0 220px; max-height:72vh; overflow:auto">
                <h3 class="section-title">Snapshots (${stamps.length})</h3>
                ${stamps
                  .map(
                    (ts) =>
                      `<div class="clickable" style="padding:4px 8px;${ts === state.playerTs ? "background:rgba(255,255,255,0.08)" : ""}" onclick="loadAt('${ts}')">${formatDate(ts)}</div>`,
                  )
                  .join("")}
            </div>
            <div style="flex:1; min-width:0">
                <h3 class="section-title">${state.playerTs ? formatDate(state.playerTs) : "Latest (reconstructed)"}</h3>
                <div class="json-viewer">${snapshot ? esc(JSON.stringify(snapshot, null, 2)) : "No snapshot"}</div>
            </div>
        </div>
    `;
}

async function loadAt(ts) {
  state.playerTs = ts;
  renderPlayerView(await api(`/players/${state.detail}/at?ts=${encodeURIComponent(ts)}`));
}

function showLatest() {
  state.playerTs = null;
  renderPlayerView(state.data.latest);
}

async function jumpToTime() {
  const v = document.getElementById("atTime").value;
  if (!v) return;
  const ms = new Date(v).getTime();
  state.playerTs = new Date(ms).toISOString();
  renderPlayerView(await api(`/players/${state.detail}/at?ts=${ms}`));
}

function renderPagination(total) {
  const start = state.offset + 1;
  const end = Math.min(state.offset + state.limit, total);
  const hasPrev = state.offset > 0;
  const hasNext = state.offset + state.limit < total;

  return `
        <div class="pagination">
            <div class="pagination-info">Showing ${start}-${end} of ${total}</div>
            <div class="pagination-buttons">
                <button ${!hasPrev ? "disabled" : ""} onclick="setOffset(${state.offset - state.limit})">Previous</button>
                <button ${!hasNext ? "disabled" : ""} onclick="setOffset(${state.offset + state.limit})">Next</button>
            </div>
        </div>
    `;
}

function doSearch() {
  const input = document.getElementById("search");
  if (input) {
    state.search = input.value;
    state.offset = 0;
    loadData();
  }
}

function render() {
  document.querySelectorAll(".nav-item").forEach((el) => {
    el.classList.toggle("active", el.dataset.page === state.page);
  });
}

document.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll(".nav-item").forEach((el) => {
    el.addEventListener("click", () => navigate(el.dataset.page));
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") document.getElementById("modal").classList.remove("open");
  });

  render();
  loadData();
});
