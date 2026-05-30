/* globals React */

function Footer() {
  const items = window.UI_DATA.integrations;
  return (
    <footer className="ui-footer">
      <div className="ui-footer-group">
        {items.map(i => (
          <div key={i.id} className="ui-footer-dot" title={i.name + " — " + (i.status === "ok" ? "connected" : "error")}>
            <iconify-icon icon={i.icon} style={i.color ? { color: i.color } : undefined}></iconify-icon>
            <span className={"ui-footer-led " + (i.status === "ok" ? "ok" : "er")}></span>
          </div>
        ))}
      </div>
      <div className="ui-footer-group right">
        <span className="ui-footer-sync">
          <iconify-icon icon="lucide:refresh-cw"></iconify-icon>
          Synced 3 min ago
        </span>
        <span className="ui-footer-sep"></span>
        <span className="ui-footer-hint">
          Press <kbd className="ui-kbd">?</kbd> for shortcuts
        </span>
      </div>
    </footer>
  );
}

Object.assign(window, { Footer });
