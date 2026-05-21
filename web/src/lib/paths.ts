export function zoneRootPath(zoneSlug: string) {
  return `/z/${zoneSlug}`
}

export function channelPath(args: { zoneSlug?: string | null; channelId: string }) {
  const { zoneSlug, channelId } = args
  if (zoneSlug) return `/z/${zoneSlug}/channel/${channelId}`
  return `/channel/${channelId}`
}

export function messagePath(args: { zoneSlug?: string | null; channelId: string; messageId: string }) {
  const { zoneSlug, channelId, messageId } = args
  if (zoneSlug) return `/z/${zoneSlug}/channel/${channelId}/msg/${messageId}`
  return `/channel/${channelId}/msg/${messageId}`
}

export function agentPath(args: { zoneSlug?: string | null; agentId: string }) {
  const { zoneSlug, agentId } = args
  if (zoneSlug) return `/z/${zoneSlug}/agent/${agentId}`
  return `/agent/${agentId}`
}

export function devtoolsPath(args: { zoneSlug?: string | null }) {
  const { zoneSlug } = args
  if (zoneSlug) return `/z/${zoneSlug}/devtools`
  return `/devtools`
}

export function daemonsPath(args: { zoneSlug?: string | null }) {
  const { zoneSlug } = args
  if (zoneSlug) return `/z/${zoneSlug}/daemons`
  return `/daemons`
}

export function daemonDetailPath(args: { zoneSlug?: string | null; machineId: string }) {
  const { zoneSlug, machineId } = args
  if (zoneSlug) return `/z/${zoneSlug}/daemons/${machineId}`
  return `/daemons/${machineId}`
}

