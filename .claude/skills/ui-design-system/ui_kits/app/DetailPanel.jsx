/* globals React, SourceBadge, Tag */

function Action({ icon, label, kbd, onClick, primary }) {
  return (
    <button className={"ui-act " + (primary ? "primary" : "")} onClick={onClick}>
      <iconify-icon icon={icon}></iconify-icon>
      <span>{label}</span>
      {kbd && <kbd className="ui-kbd">{kbd}</kbd>}
    </button>
  );
}

function DetailPanel({ n, onComplete, onDelete, onSnooze }) {
  if (!n) {
    return (
      <section className="ui-detail empty">
        <iconify-icon icon="lucide:inbox"></iconify-icon>
        <div className="ui-detail-empty-t">Select a notification</div>
        <div className="ui-detail-empty-s">Pick a notification on the left to preview it here.</div>
      </section>
    );
  }

  return (
    <section className="ui-detail">
      <header className="ui-detail-head">
        <SourceBadge source={n.source} />
        <div className="ui-detail-meta">
          <div className="ui-detail-src">{n.repo}</div>
          <h1 className="ui-detail-title">{n.title}</h1>
        </div>
        <div className="ui-detail-head-actions">
          <button className="ui-iconbtn" title="Open in browser"><iconify-icon icon="lucide:external-link"></iconify-icon></button>
          <button className="ui-iconbtn" title="Copy link"><iconify-icon icon="lucide:link"></iconify-icon></button>
          <button className="ui-iconbtn" title="More"><iconify-icon icon="lucide:more-horizontal"></iconify-icon></button>
        </div>
      </header>

      <div className="ui-detail-chips">
        {n.tag && <Tag tag={n.tag} />}
        {n.mention && <span className="ui-tag ui-tag-mention">mention</span>}
        <span className="ui-tag ui-tag-neutral">
          <iconify-icon icon="lucide:user"></iconify-icon>
          {n.actor}
        </span>
        <span className="ui-tag ui-tag-neutral">
          <iconify-icon icon="lucide:clock"></iconify-icon>
          {n.when} ago
        </span>
      </div>

      <div className="ui-detail-body">
        {n.body.split("\n\n").map((para, i) => (
          <p key={i}>{para}</p>
        ))}

        <div className="ui-detail-meta-grid">
          {n.meta?.map(([k, v]) => (
            <div key={k} className="ui-detail-meta-row">
              <span className="ui-detail-meta-k">{k}</span>
              <span className="ui-detail-meta-v">{v}</span>
            </div>
          ))}
        </div>
      </div>

      <footer className="ui-dock">
        <Action icon="lucide:check"        label="Done"        kbd="c" primary onClick={() => onComplete(n.id)} />
        <Action icon="lucide:clock"        label="Snooze"      kbd="s" onClick={() => onSnooze(n.id)} />
        <Action icon="lucide:square-check" label="Create task" kbd="t" />
        <Action icon="lucide:bell-off"     label="Unsubscribe" kbd="u" />
        <div className="ui-dock-spacer" />
        <Action icon="lucide:trash-2"      label="Delete"      kbd="d" onClick={() => onDelete(n.id)} />
      </footer>
    </section>
  );
}

Object.assign(window, { DetailPanel });
