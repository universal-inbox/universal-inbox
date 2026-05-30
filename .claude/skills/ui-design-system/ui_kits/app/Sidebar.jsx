/* globals React */
const { useState } = React;

function Sidebar({ page, setPage, listFilter }) {
  const counts = { inbox: 24, snoozed: 3, done: 0, tasks: 5 };
  const Item = ({ id, icon, label, badge, badgeKind = "m" }) => (
    <button className={"ui-sb-item " + (page === id ? "active" : "")} onClick={() => setPage(id)}>
      <iconify-icon icon={icon}></iconify-icon>
      <span className="lbl">{label}</span>
      {badge != null && badge > 0 && <span className={"ui-sb-badge " + badgeKind}>{badge}</span>}
    </button>
  );

  return (
    <aside className="ui-sidebar">
      <div className="ui-sb-brand">
        <img src="../../assets/logo.svg" alt="" />
        <span>Universal Inbox</span>
      </div>

      <div className="ui-sb-section">
        <div className="ui-sb-label">Inbox</div>
        <Item id="inbox"   icon="lucide:inbox"        label="All Notifications" badge={counts.inbox}   badgeKind="p" />
        <Item id="snoozed" icon="lucide:clock"        label="Snoozed"           badge={counts.snoozed} />
        <Item id="done"    icon="lucide:check"        label="Done" />
      </div>

      <div className="ui-sb-section">
        <div className="ui-sb-label">Tasks</div>
        <Item id="tasks"   icon="lucide:square-check" label="Synced Tasks"      badge={counts.tasks} />
      </div>

      <div className="ui-sb-section">
        <div className="ui-sb-label">Manage</div>
        <Item id="settings" icon="lucide:settings" label="Settings" />
      </div>

      <div className="ui-sb-foot">
        <div className="ui-sb-avatar">DR</div>
        <div className="ui-sb-me">
          <div className="ui-sb-me-name">David Rousselié</div>
          <div className="ui-sb-me-sub">david@universalinbox.com</div>
        </div>
        <button className="ui-sb-more" aria-label="More">
          <iconify-icon icon="lucide:more-horizontal"></iconify-icon>
        </button>
      </div>
    </aside>
  );
}

Object.assign(window, { Sidebar });
