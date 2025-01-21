import React, { useRef, useState, useEffect } from 'react';
import ChatWindow from './components/ChatWindow';
import SideBar from './components/SideBar';
import { invoke } from '@tauri-apps/api/tauri';

const App = () => {
  const chatWindowRef = useRef();
  const [sessions, setSessions] = useState([]);
  const [currentSessionId, setCurrentSessionId] = useState(null);

  const loadCurrentSession = async () => {
    try {
      const currentSession = await invoke('get_current_session');
      setSessions((prevSessions) => {
        const exists = prevSessions.some(session => session.id === currentSession.id);
        if (!exists) {
          return [currentSession, ...prevSessions];
        }
        return prevSessions;
      });
    } catch (error) {
      console.error('Failed to load current session:', error);
    }
  };

  const handleSessionSelect = (sessionId) => {
    setCurrentSessionId(sessionId);
  };

  const clearChat = () => {
    chatWindowRef.current?.clearChat();
  };

  const handleNewSession = async () => {
    try {
      const currentSession = await invoke('get_current_session');

      setSessions((prevSessions) => [
        currentSession,
        ...prevSessions.filter(session => session.id !== currentSession.id),
      ]);

      setCurrentSessionId(currentSession.id);

    } catch (error) {
      console.error('Failed to start a new session:', error);
    }
  };

  // className="flex flex-row h-screen h-full w-full p-2 box-border"

  return (
    <div className="flex h-screen h-full w-full p-2 box-border">
      <SideBar
        className="flex-none w-1/3 max-w-[30%]"
        sessions={sessions}
        setSessions={setSessions}
        onSessionSelect={setCurrentSessionId}
        onNewSession={handleNewSession}
        currentSessionId={currentSessionId}
        clearChat={clearChat} />
      <ChatWindow
        className="flex-grow w-2/3 max-w-[70%]"
        ref={chatWindowRef}
        currentSessionId={currentSessionId}
        refreshSessions={loadCurrentSession}
        onNewSession={handleNewSession} />
    </div>
  );
};

export default App;