import type { GraphCommit } from './types';

export const LANE_COLORS = [
  '#3b82f6', // blue
  '#f97316', // orange
  '#a855f7', // purple
  '#22c55e', // green
  '#06b6d4', // cyan
  '#ec4899', // pink
];

export interface GraphEdge {
  fromLane: number;
  toLane: number;
  color: string;
}

export interface GraphRowLayout {
  commit: GraphCommit;
  lane: number;
  color: string;
  maxLanes: number;
  incoming: GraphEdge[];
  outgoing: GraphEdge[];
}

/** Computes VS Code-like commit graph lanes and parent edges in a single top-to-bottom pass. */
export function layoutGraph(commits: GraphCommit[]): GraphRowLayout[] {
  const active: (string | null)[] = [];
  const laneColor = (lane: number) => LANE_COLORS[lane % LANE_COLORS.length];
  const rows: GraphRowLayout[] = [];

  for (const commit of commits) {
    let lane = active.indexOf(commit.hash);
    if (lane === -1) {
      lane = active.indexOf(null);
      if (lane === -1) {
        lane = active.length;
        active.push(commit.hash);
      } else {
        active[lane] = commit.hash;
      }
    }

    const incoming: GraphEdge[] = [];
    for (let i = 0; i < active.length; i++) {
      const h = active[i];
      if (!h) continue;
      if (h === commit.hash) {
        incoming.push({ fromLane: i, toLane: lane, color: laneColor(i) });
      } else {
        incoming.push({ fromLane: i, toLane: i, color: laneColor(i) });
      }
    }

    // Clear all slots pointing to this commit
    for (let i = 0; i < active.length; i++) {
      if (active[i] === commit.hash) active[i] = null;
    }

    const outgoing: GraphEdge[] = [];
    // Keep pass-through lanes
    for (let i = 0; i < active.length; i++) {
      if (active[i]) {
        outgoing.push({ fromLane: i, toLane: i, color: laneColor(i) });
      }
    }

    // Place parents
    commit.parents.forEach((parentHash, idx) => {
      let targetLane = active.indexOf(parentHash);
      if (targetLane === -1) {
        if (idx === 0 && active[lane] === null) {
          targetLane = lane;
        } else {
          targetLane = active.indexOf(null);
          if (targetLane === -1) {
            targetLane = active.length;
            active.push(null);
          }
        }
        active[targetLane] = parentHash;
      }
      outgoing.push({
        fromLane: lane,
        toLane: targetLane,
        color: laneColor(idx === 0 ? lane : targetLane),
      });
    });

    while (active.length > 0 && active[active.length - 1] === null) {
      active.pop();
    }

    const maxLanes = Math.max(
      lane + 1,
      active.length,
      ...incoming.map((e) => Math.max(e.fromLane, e.toLane) + 1),
      ...outgoing.map((e) => Math.max(e.fromLane, e.toLane) + 1),
    );

    rows.push({
      commit,
      lane,
      color: laneColor(lane),
      maxLanes: Math.min(maxLanes, 6),
      incoming,
      outgoing,
    });
  }

  return rows;
}
