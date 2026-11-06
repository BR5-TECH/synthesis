/**
 * Where the shell mounts the standalone confirmation (MBW-FR-GYVD,
 * SNV-FR-56). It follows the request store and mounts one dialog per request.
 */
import { useSyncExternalStore } from "react";

import {
  currentMainBranchWrite,
  subscribeMainBranchWrite,
} from "../../state/mainBranchWrite";
import { MainBranchWriteDialog } from "./MainBranchWriteDialog";

export function MainBranchWriteHost() {
  const request = useSyncExternalStore(subscribeMainBranchWrite, currentMainBranchWrite);
  if (!request) return null;
  return <MainBranchWriteDialog key={request.id} request={request} />;
}
