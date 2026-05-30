/* globals React */
const { useMemo } = React;

function SourceBadge({ source }) {
  const i = (window.UI_DATA.integrations.find(x => x.id === source)) || { icon: "lucide:bell", color: "#64748B" };
  return (
    <div className="ui-src">
      <iconify-icon icon={i.icon} style={i.color ? { color: i.color } : undefined}></iconify-icon>
    </div>
  );
}

function Tag({ tag }) {
  if (!tag) return null;
  return <span className={"ui-tag ui-tag-" + tag.kind}>{tag.label}</span>;
}

function NotifRow({ n, selected, onSelect }) {
  return (
    <button
      className={"ui-nrow " + (n.unread ? "unread " : "") + (selected ? "selected" : "")}
      onClick={() => onSelect(n.id)}
    >
      <SourceBadge source={n.source} />
      <div className="ui-nrow-body">
        <div className="ui-nrow-r1">
          <span className={"ui-nrow-title" + (n.unread ? " b" : "")}>{n.title}</span>
          {n.unread && <span className="ui-unread-dot"></span>}
          <span className="ui-nrow-time">{n.when}</span>
        </div>
        <div className="ui-nrow-r2">
          <span className={"ui-nrow-src" + (n.unread ? " b" : "")}>{n.repo}</span>
          <Tag tag={n.tag} />
          {n.mention && <span className="ui-tag ui-tag-mention">mention</span>}
        </div>
      </div>
    </button>
  );
}

function ListPanel({ notifications, selectedId, onSelect, filter, setFilter, query, setQuery }) {
  const filtered = useMemo(() => {
    let arr = notifications;
    if (filter === "unread") arr = arr.filter(n => n.unread);
    if (filter === "mentions") arr = arr.filter(n => n.mention);
    if (query) {
      const q = query.toLowerCase();
      arr = arr.filter(n => (n.title + " " + n.repo).toLowerCase().includes(q));
    }
    return arr;
  }, [notifications, filter, query]);

  const grouped = useMemo(() => {
    const map = {};
    for (const n of filtered) (map[n.group] ||= []).push(n);
    return map;
  }, [filtered]);

  const counts = {
    all: notifications.length,
    unread: notifications.filter(n => n.unread).length,
    mentions: notifications.filter(n => n.mention).length,
  };

  return (
    <section className="ui-list">
      <header className="ui-list-head">
        <div className="ui-list-title-row">
          <h2 className="ui-list-title">All Notifications</h2>
          <div className="ui-list-actions">
            <button className="ui-iconbtn" aria-label="Refresh"><iconify-icon icon="lucide:refresh-cw"></iconify-icon></button>
            <button className="ui-iconbtn" aria-label="Filter"><iconify-icon icon="lucide:sliders-horizontal"></iconify-icon></button>
          </div>
        </div>

        <div className="ui-search">
          <iconify-icon icon="lucide:search"></iconify-icon>
          <input
            placeholder="Search notifications…"
            value={query}
            onChange={e => setQuery(e.target.value)}
          />
          <kbd className="ui-kbd">/</kbd>
        </div>

        <div className="ui-chips">
          {[
            ["all", "All", counts.all],
            ["unread", "Unread", counts.unread],
            ["mentions", "Mentions", counts.mentions],
          ].map(([k, l, c]) => (
            <button key={k} className={"ui-chip " + (filter === k ? "active" : "")} onClick={() => setFilter(k)}>
              {l} <span className="c">{c}</span>
            </button>
          ))}
        </div>
      </header>

      <div className="ui-list-scroll">
        {Object.keys(grouped).length === 0 && (
          <div className="ui-empty">
            <iconify-icon icon="lucide:inbox"></iconify-icon>
            <div className="ui-empty-t">You're all caught up.</div>
            <div className="ui-empty-s">Nothing needs your attention right now. New notifications will appear here as they arrive.</div>
          </div>
        )}
        {Object.entries(grouped).map(([group, rows]) => (
          <div key={group}>
            <div className="ui-group-label">{group}</div>
            {rows.map(n => (
              <NotifRow key={n.id} n={n} selected={selectedId === n.id} onSelect={onSelect} />
            ))}
          </div>
        ))}
      </div>
    </section>
  );
}

Object.assign(window, { ListPanel, SourceBadge, Tag });
