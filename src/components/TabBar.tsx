import { closeTab, newTab } from '@/state/actions'
import { useStore, useT } from '@/state/store'

export function TabBar() {
  const t = useT()
  const tabs = useStore((s) => s.tabs)
  const activeTabId = useStore((s) => s.activeTabId)
  const setActiveTab = useStore((s) => s.setActiveTab)

  return (
    <div className="tabbar">
      {tabs.map((tab) => {
        const title = tab.connecting
          ? t('tab.connecting')
          : tab.site
            ? tab.site.name || tab.site.host
            : t('tab.newTab')

        const color = tab.site?.color ?? null
        const active = tab.id === activeTabId

        return (
          <div
            key={tab.id}
            className={`tab${active ? ' active' : ''}`}
            style={
              color
                ? {
                  borderTop: `3px solid ${color}`,
                  background: active
                    ? `color-mix(in srgb, ${color} 18%, var(--bg-elev))`
                    : undefined
                }
                : undefined
            }
            onClick={() => setActiveTab(tab.id)}
            onMouseDown={(e) => {
              if (e.button === 1) {
                e.preventDefault()
                void closeTab(tab.id)
              }
            }}
            title={tab.site ? `${tab.site.host}:${tab.site.port}` : undefined}
          >
            <span
              className="dot"
              style={{
                background: tab.sessionId
                  ? 'var(--success)'
                  : tab.connecting
                    ? 'var(--warning)'
                    : 'var(--text-dim)'
              }}
            />
            <span className="title">{title}</span>
            <button
              className="close"
              onClick={(e) => {
                e.stopPropagation()
                void closeTab(tab.id)
              }}
              title={t('menu.closeTab')}
            >
              ×
            </button>
          </div>
        )
      })}
      <button
        className="add ghost"
        onClick={() => void newTab()}
        title={`${t('menu.newTab')} (Ctrl+T)`}
      >
        +
      </button>
    </div>
  )
}