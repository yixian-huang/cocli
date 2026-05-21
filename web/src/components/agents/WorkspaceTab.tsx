import { useState, useEffect, useRef, useCallback, type ReactElement } from 'react'
import { agentWorkspace } from '@/api/client'
import { Folder, File, ChevronRight, ChevronDown, Loader2, AlertCircle } from 'lucide-react'
import { cn } from '@/lib/utils'

interface FileEntry {
  name: string
  isDir: boolean
  size: number
}

interface DirState {
  files: FileEntry[]
  expanded: boolean
  loaded: boolean
}

export function WorkspaceTab({ agentId, offline }: { agentId: string; offline: boolean }) {
  const [dirs, setDirs] = useState<Map<string, DirState>>(new Map())
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState<string>('')
  const [fileBinary, setFileBinary] = useState(false)
  const [loadingFile, setLoadingFile] = useState(false)
  const [unavailable, setUnavailable] = useState(false)
  const agentIdRef = useRef(agentId)
  agentIdRef.current = agentId

  const loadDir = useCallback(async (path: string) => {
    const id = agentIdRef.current
    try {
      const res = await agentWorkspace.listDir(id, path)
      setDirs((prev) => {
        const next = new Map(prev)
        next.set(path, { files: res.files || [], expanded: true, loaded: true })
        return next
      })
    } catch {
      if (path === '/') setUnavailable(true)
    }
  }, [])

  useEffect(() => {
    if (!offline) {
      setDirs(new Map())
      setUnavailable(false)
      loadDir('/')
    }
  }, [agentId, offline, loadDir])

  const toggleDir = (path: string) => {
    const dir = dirs.get(path)
    if (!dir) {
      loadDir(path)
    } else {
      setDirs((prev) => {
        const next = new Map(prev)
        next.set(path, { ...dir, expanded: !dir.expanded })
        return next
      })
    }
  }

  const openFile = async (path: string) => {
    setSelectedFile(path)
    setLoadingFile(true)
    try {
      const res = await agentWorkspace.readFile(agentId, path)
      setFileContent(res.content)
      setFileBinary(res.binary)
    } catch {
      setFileContent('Error loading file')
      setFileBinary(false)
    } finally {
      setLoadingFile(false)
    }
  }

  if (offline || unavailable) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-2">
        <AlertCircle className="h-8 w-8 opacity-40" />
        <span className="text-sm">Agent {offline ? 'offline' : 'unreachable'}, workspace unavailable</span>
      </div>
    )
  }

  const renderTree = (path: string, depth: number): ReactElement[] => {
    const dir = dirs.get(path)
    if (!dir || !dir.expanded) return []

    return dir.files.map((f) => {
      const fullPath = path === '/' ? `/${f.name}` : `${path}/${f.name}`
      if (f.isDir) {
        const subDir = dirs.get(fullPath)
        return (
          <div key={fullPath}>
            <button
              onClick={() => toggleDir(fullPath)}
              className="flex items-center gap-1 w-full px-2 py-1 text-xs hover:bg-accent/50 rounded"
              style={{ paddingLeft: `${depth * 16 + 8}px` }}
            >
              {subDir?.expanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
              <Folder className="h-3 w-3 text-blue-400" />
              <span className="truncate">{f.name}</span>
            </button>
            {renderTree(fullPath, depth + 1)}
          </div>
        )
      }
      return (
        <button
          key={fullPath}
          onClick={() => openFile(fullPath)}
          className={cn(
            'flex items-center gap-1 w-full px-2 py-1 text-xs hover:bg-accent/50 rounded',
            selectedFile === fullPath && 'bg-accent',
          )}
          style={{ paddingLeft: `${depth * 16 + 20}px` }}
        >
          <File className="h-3 w-3 text-muted-foreground" />
          <span className="truncate">{f.name}</span>
        </button>
      )
    })
  }

  return (
    <div className="flex-1 flex min-h-0">
      <div className="w-56 border-r overflow-y-auto shrink-0 py-1">
        {renderTree('/', 0)}
      </div>
      <div className="flex-1 overflow-auto p-3">
        {selectedFile ? (
          loadingFile ? (
            <div className="flex items-center justify-center h-full">
              <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
            </div>
          ) : fileBinary ? (
            <div className="text-sm text-muted-foreground text-center mt-8">Binary file — cannot display</div>
          ) : (
            <div>
              <div className="text-xs text-muted-foreground mb-2 font-mono">{selectedFile}</div>
              <pre className="text-xs font-mono whitespace-pre-wrap break-all bg-muted/30 rounded p-3">{fileContent}</pre>
            </div>
          )
        ) : (
          <div className="text-sm text-muted-foreground text-center mt-8">Select a file to view</div>
        )}
      </div>
    </div>
  )
}
