import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import newChatIcon from './assets/new_chat.png';

const SideBar = ({ className, sessions, setSessions, onSessionSelect, currentSessionId, clearChat }) => {
    const [contextMenu, setContextMenu] = useState({ visible: false, x: 0, y: 0, sessionId: null });
    const [editingSessionId, setEditingSessionId] = useState(null);
    const [editedTitle, setEditedTitle] = useState("");

    // Load chat sessions on startup
    useEffect(() => {
        loadChatSessions();
    }, []);

    // function to clear chat and start a new chat
    const handleNewChat = async () => {
        try {
            await invoke('clear_current_session');
            onSessionSelect(null);
            clearChat();
            console.log('Cleared current session');
        } catch (error) {
            console.error('Failed to clear the current session:', error);
        }
    };

    // Function to load all chat sessions
    const loadChatSessions = async () => {
        try {
            const loadedSessions = await invoke('load_chat_sessions');
            console.log('Loaded Sessions:', loadedSessions);
            setSessions(loadedSessions);
        } catch (error) {
            console.error('Failed to load sessions:', error);
        }
    };

    // Handle session click and notify parent
    const handleSessionClick = async (sessionId) => {
        try {
            // Set the clicked session as the current session in Rust
            await invoke('set_current_session', { sessionId });

            // Notify ChatWindow to load chat history for this session
            onSessionSelect(sessionId);
        } catch (error) {
            console.error('Failed to switch session:', error);
        }
    };

    const handleRightClick = (e, sessionId, sessionTitle) => {
        e.preventDefault();
        setContextMenu({
            visible: true,
            x: e.pageX,
            y: e.pageY,
            sessionId: sessionId
        });
        setEditedTitle(sessionTitle);
    };

    const handleDelete = async (sessionId) => {
        try {
            await invoke('delete_chat_session', { sessionId });
            loadChatSessions();

            // if current, clear ChatWindow history
            if (sessionId === currentSessionId) {
                console.log('Clearing current chat history')
                onSessionSelect(null);  // Deselect the current session
            }
        } catch (error) {
            console.error('Failed to delete session:', error);
        } finally {
            // Hide the context menu after deletion
            setContextMenu({ visible: false, x: 0, y: 0, sessionId: null });
        }
    };

    const handleRename = async (sessionId) => {
        try {
            await invoke('update_chat_session_name', { sessionId, newName: editedTitle });
            loadChatSessions();  // Refresh the session list
        } catch (error) {
            console.error('Failed to rename session:', error);
        } finally {
            // Hide the context menu after renaming
            setEditingSessionId(null);
            setContextMenu({ visible: false, x: 0, y: 0, sessionId: null });
        }
    };

    return (
        <div className="${className} w-64 bg-gray-200 border-r overflow-y-auto min-w-0 p-4 box-border h-full">
            <div className="flex justify-between items-center mb-4">
                <h3 className="text-lg font-bold text-gray-800">Sessions</h3>
                <button
                    className="w-8 h-8 flex items-center justify-center hover:bg-gray-300 rounded hover:scale-110 transition-transform"
                >
                    <img
                        src={newChatIcon}
                        alt="New Chat"
                        className="w-5 h-5 object-contain"
                        onClick={handleNewChat}
                    />
                </button>
            </div>
            <ul className="list-none p-0 m-0">
                {(sessions || []).map((session) => (
                    <li
                        key={session.id}
                        onClick={() => handleSessionClick(session.id)}
                        onContextMenu={(e) => handleRightClick(e, session.id, session.title)}
                        className={
                            session.id === currentSessionId ?
                                'bg-blue-100 font-bold mb-1 p-2 text-sm text-left' :
                                'cursor-pointer mb-1 p-2 hover:bg-gray-300 text-sm text-left'}
                    >
                        {editingSessionId === session.id ? (
                            <input
                                type="text"
                                value={editedTitle}
                                onChange={(e) => setEditedTitle(e.target.value)}
                                onBlur={() => handleRename(session.id)}
                                onKeyDown={(e) => {
                                    if (e.key === 'Enter') {
                                        handleRename(session.id);  // Save on Enter
                                    } else if (e.key === 'Escape') {
                                        setEditingSessionId(null);  // Cancel on Escape
                                    }
                                }}
                                autoFocus
                            />
                        ) : (
                            session.title
                        )}
                    </li>
                ))}
            </ul>
            {contextMenu.visible && (
                <div
                    className="absolute bg-white border border-gray-300 p-2 z-50 shadow-md rounded-lg"
                    style={{
                        position: 'absolute',
                        top: contextMenu.y,
                        left: contextMenu.x
                    }}
                    onMouseLeave={() => setContextMenu({ visible: false, x: 0, y: 0, sessionId: null })}
                >
                    <div
                        className="px-2 py-1 cursor-pointer text-sm hover:bg-gray-200 transition-colors"
                        onClick={() => {
                            setEditingSessionId(contextMenu.sessionId);
                            setContextMenu({ visible: false, x: 0, y: 0, sessionId: null });
                        }}>
                        Rename
                    </div>
                    <div
                        className="px-2 py-1 cursor-pointer text-sm hover:bg-gray-200 transition-colors"
                        onClick={() => {
                            handleDelete(contextMenu.sessionId);
                            setContextMenu({ visible: false, x: 0, y: 0, sessionId: null });
                        }}>
                        Delete
                    </div>
                </div>
            )}
        </div>
    );
};

export default SideBar;