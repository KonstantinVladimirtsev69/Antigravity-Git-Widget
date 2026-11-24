export const IS_T3_HOST =
  new URLSearchParams(window.location.search).get('host') === 't3code';

export async function deliverPromptToHost(
  prompt: string,
  alreadySent: boolean,
  sentLabel = 'Sent to AI chat',
): Promise<string> {
  if (alreadySent) return sentLabel;
  const trimmed = prompt.trim();
  if (!trimmed) return 'Empty prompt ignored';
  if (IS_T3_HOST && window.parent && window.parent !== window) {
    window.parent.postMessage(
      { type: 'git-widget-ask-ai', prompt: trimmed },
      window.location.origin,
    );
    return 'Prompt sent to T3 Code composer';
  }
  await navigator.clipboard.writeText(trimmed);
  return 'Copied to clipboard (direct chat send unavailable)';
}
