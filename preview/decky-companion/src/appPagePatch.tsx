import { routerHook } from '@decky/api';
import { afterPatch, wrapReactType, type Patch } from '@decky/ui';
import type { ReactElement } from 'react';

const NON_STEAM_APP = 1073741824;
const SECTION_ID = 'achievement-watcher-poc';

function ProofOfConcept({ appId }: { appId: number }) {
  return (
    <section
      id={SECTION_ID}
      style={{
        margin: '16px 0',
        padding: '18px 20px',
        background: 'rgba(14, 20, 27, 0.92)',
        borderLeft: '4px solid #66c0f4',
        color: '#f5f5f5',
      }}
    >
      <div style={{ fontSize: 20, fontWeight: 600 }}>Achievement Watcher</div>
      <div style={{ marginTop: 5, color: '#acb2b8', fontSize: 14 }}>
        Game page connection working · App {appId}
      </div>
    </section>
  );
}

export function patchAppPage() {
  const nestedPatches = new Set<Patch>();
  const patchedOwners = new Set<Record<string, any>>();
  const routePatch = routerHook.addPatch(
    '/library/app/:appid',
    (props: { path?: string; children?: ReactElement }) => {
      const renderOwner = props.children?.props;
      if (!renderOwner?.renderFunc || renderOwner.renderFunc.__achievementWatcherPatched) {
        return props;
      }

      const renderPatch = afterPatch(renderOwner, 'renderFunc', (_args, page: ReactElement) => {
        const overview = page?.props?.children?.props?.overview;
        if (overview?.app_type !== NON_STEAM_APP) return page;

        const contentOwner = wrapReactType(page.props.children);
        if (contentOwner.__achievementWatcherPatched) return page;
        const contentPatch = afterPatch(contentOwner, 'type', (_innerArgs, content) => {
          const sections = content?.props?.children?.[1]?.props?.children?.props?.children;
          if (!Array.isArray(sections) || sections.some((child) => child?.props?.id === SECTION_ID)) {
            return content;
          }

          sections.unshift(<ProofOfConcept appId={overview.appid} />);
          return content;
        });
        contentOwner.__achievementWatcherPatched = true;
        patchedOwners.add(contentOwner);
        nestedPatches.add(contentPatch);
        return page;
      });
      renderOwner.renderFunc.__achievementWatcherPatched = true;
      patchedOwners.add(renderOwner.renderFunc);
      nestedPatches.add(renderPatch);
      return props;
    },
  );

  return () => {
    routerHook.removePatch('/library/app/:appid', routePatch);
    for (const patch of nestedPatches) {
      if (!patch.hasUnpatched) patch.unpatch();
    }
    for (const owner of patchedOwners) delete owner.__achievementWatcherPatched;
  };
}
