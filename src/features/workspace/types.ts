import type { Conversation, Pane } from '../chat/types'

export interface Preferences {
  codexPath: string
  nativeLanguage: string
  targetLanguage: string
  mainModel: string
  tutorModel: string
  mainId: string | null
  tutorId: string | null
  tutorMode: string
  activeView: 'conversation' | 'vocabulary'
  mobilePane: Pane
}
export interface Workspace {
  preferences: Preferences
  history: Conversation[]
  main: Conversation | null
  tutor: Conversation | null
  path: string
}
