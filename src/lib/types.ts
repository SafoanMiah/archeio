export interface LiveState {
  is_running: boolean;
  started_at: string | null;
}

export interface Broadcast {
  id: string;
  started_at: string;
  ended_at: string | null;
  youtube_video_id: string | null;
  title: string | null;
}

export interface OAuthStatus {
  connected: boolean;
  client_present: boolean;
}
