import { useCallback, useEffect, useState } from "react";

const COPIED_MS = 2000;

/** Copies the current URL (it carries region, mode, month and language); `copied` stays true for 2 s. */
export function useCopyLink(): { copied: boolean; copyLink: () => Promise<void> } {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const id = window.setTimeout(() => setCopied(false), COPIED_MS);
    return () => window.clearTimeout(id);
  }, [copied]);

  const copyLink = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      setCopied(true);
    } catch {
      // Clipboard can be blocked (insecure context, permissions); the URL bar still has the link.
    }
  }, []);

  return { copied, copyLink };
}
