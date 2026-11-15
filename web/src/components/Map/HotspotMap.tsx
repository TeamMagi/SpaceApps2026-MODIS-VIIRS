import { Component, lazy, memo, Suspense, useCallback, useState, type ReactNode } from "react";
import { useCopy } from "../../hooks/useCopy";
import FlatMap from "./FlatMap";
import MapStateOverlay from "./MapStateOverlay";
import styles from "./HotspotMap.module.css";

// three.js lives only in this chunk, so the flat fallback never downloads it.
const GlobeMap = lazy(() => import("./GlobeMap"));

function supportsWebGL2(): boolean {
  if (typeof document === "undefined") return false;
  try {
    const gl = document.createElement("canvas").getContext("webgl2");
    if (!gl) return false;
    gl.getExtension("WEBGL_lose_context")?.loseContext();
    return true;
  } catch {
    return false;
  }
}

interface GlobeBoundaryProps {
  readonly onError: () => void;
  readonly children: ReactNode;
}

/** A failed chunk load or a WebGL error while rendering drops to the flat map instead of a blank panel. */
class GlobeBoundary extends Component<GlobeBoundaryProps, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(error: unknown): void {
    console.warn("Globe map failed; using the flat map.", error);
    this.props.onError();
  }

  render(): ReactNode {
    return this.state.failed ? null : this.props.children;
  }
}

function GlobeLoading() {
  const { t } = useCopy();
  return (
    <section className={styles.map} data-surface="globe" aria-label={t("mapLabel")}>
      <div className={styles.viewport}>
        <MapStateOverlay state="loading" />
      </div>
    </section>
  );
}

function HotspotMap() {
  const [globe, setGlobe] = useState(supportsWebGL2);
  const showFlat = useCallback(() => setGlobe(false), []);

  if (!globe) return <FlatMap />;
  return (
    <GlobeBoundary onError={showFlat}>
      <Suspense fallback={<GlobeLoading />}>
        <GlobeMap onUnsupported={showFlat} />
      </Suspense>
    </GlobeBoundary>
  );
}

export default memo(HotspotMap);
