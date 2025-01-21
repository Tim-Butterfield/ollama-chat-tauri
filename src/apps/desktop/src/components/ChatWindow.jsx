import React, { useState, useEffect, useRef, useImperativeHandle, forwardRef } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import ReactMarkdown from "react-markdown";

const ChatWindow = forwardRef(({ className, currentSessionId, refreshSessions, onNewSession }, ref) => {
    const [models, setModels] = useState([]);
    const [selectedModel, setSelectedModel] = useState("Select a model:tag");
    const [userInput, setUserInput] = useState("");
    const [chatHistory, setChatHistory] = useState([]);
    const [loading, setLoading] = useState(false);
    const chatEndRef = useRef(null);
    const [windowHeight, setWindowHeight] = useState(window.innerHeight);

    // Load models and chat history on startup
    useEffect(() => {
        const loadInitialData = async () => {
            try {
                await loadModels(); // Ensure models are loaded
                loadChatHistory();  // Independent task
            } catch (error) {
                console.error("Error during initialization:", error);
            }
        };

        loadInitialData();
    }, []);

    // Second effect triggers only when models is updated
    useEffect(() => {
        if (models.length > 0) {
            loadDefaultModel();
        }
    }, [models]);

    // if a new chat is started, refresh the list of chat sessions
    useEffect(() => {
        if (chatHistory.length === 1) {
            refreshSessions();
        }
    }, [chatHistory]);

    // when a chat session title is selected, load the history for that chat
    useEffect(() => {
        if (currentSessionId !== null && currentSessionId !== -1) {
            loadChatHistory();
        } else {
            setChatHistory([]);
        }
    }, [currentSessionId]);

    // Auto-scroll on new message
    useEffect(() => {
        scrollToBottom();
    }, [chatHistory]);

    // Adjust height on window resize
    useEffect(() => {
        const handleResize = () => setWindowHeight(window.innerHeight);
        window.addEventListener("resize", handleResize);

        return () => window.removeEventListener("resize", handleResize);
    }, []);

    // Auto-scroll to the bottom
    const scrollToBottom = () => {
        chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
    };

    // Load Ollama models
    const loadModels = async () => {
        if (models.length === 0) {
            try {
                const models = await invoke("load_models");
                setModels(models);
            } catch (error) {
                console.error("Failed to load models:", error);
            }
        }
    };

    // Load previously selected model
    const loadDefaultModel = async () => {
        if (models.length > 0) {
            try {
                const savedModel = await invoke("get_selected_model");

                // If savedModel is null or invalid, default to the first model in the models list
                if (!savedModel || savedModel.trim() === "") {
                    const defaultModel = models[0];
                    setSelectedModel(defaultModel);
                    saveModelSelection(defaultModel);
                } else {
                    setSelectedModel(savedModel);
                }
            } catch (error) {
                console.error("Failed to load default model:", error);
            }
        }
    };

    // Save the selected model to the backend
    const saveModelSelection = async (model) => {
        try {
            await invoke("save_selected_model", { modelName: model });
        } catch (error) {
            console.error("Failed to save model selection:", error);
        }
    };

    // Handle model change and save selection as default
    const handleModelChange = (event) => {
        const newModel = event.target.value;
        setSelectedModel(newModel);
        saveModelSelection(newModel);
    };

    React.useImperativeHandle(ref, () => ({
        clearChat: () => setChatHistory([]),
    }));

    // Load chat history
    const loadChatHistory = async () => {
        try {
            const history = await invoke("load_chat_history");

            // Check if the history is empty or not an array
            if (!history || !Array.isArray(history) || history.length === 0) {
                console.log("Chat history is empty. Nothing to load.");
                return;  // Do not update chatHistory if empty
            }

            // Map 'sender' to 'role' to match UI expectations
            const formattedHistory = history.map((message) => ({
                role: message.role === "user" ? "user" : "ai",
                content: message.content,
            }));

            setChatHistory(formattedHistory);
            console.log("Loaded Chat History:", formattedHistory);

        } catch (error) {
            console.error("Failed to load chat history:", error);
        }
    };

    // Send prompt
    const sendMessage = async () => {
        if (!userInput.trim()) return;

        if (!selectedModel) {
            alert("No model selected");
            return;
        }

        const emptyChatHistory = (chatHistory.length === 0);
        setLoading(true);

        try {
            const response = await invoke("generate_chat", {
                prompt: userInput,
                model: selectedModel,
            });

            console.log("AI Response:", response);

            // Update the chat history with the user's input and AI's response
            setChatHistory((prevHistory) => {
                const updatedHistory = [
                    ...prevHistory,
                    { role: "user", content: userInput },   // User's message
                    { role: "ai", content: response }       // AI's response
                ];

                console.log("Updated Chat History:", updatedHistory);

                return updatedHistory;
            });

            // if starting a new chat, update sidebar
            if (emptyChatHistory) {
                invoke("get_current_session").then((newSession) => {
                    onNewSession(newSession);  // Prepend the session in SideBar
                });
            }

            // Clear user input after sending
            setUserInput("");
        } catch (error) {
            console.error("Error generating chat:", error);
            alert("Failed to generate a response.");
        } finally {
            setLoading(false);
        }
    };

    // Abort ongoing generation
    const handleAbort = async () => {
        await invoke("abort_generation");
        setLoading(false);
    };

    return (
        <div
            className="${className} w-full flex flex-col flex-grow overflow-y-auto h-full bg-white border shadow-lg p-4 box-border overflow-hidden"
        >
            {/* Model selection drop-down */}
            <div className="flex justify-between items-center mb-2">
                <div className="pb-2 flex items-center space-x-4">
                    <label
                        htmlFor="model"
                        className="text-sm font-medium text-gray-700"
                    >Select a Model:</label>
                    <select
                        id="model"
                        name="model"
                        className="px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
                        value={selectedModel}
                        onChange={handleModelChange}
                    >
                        <option disabled>Select a model...</option>
                        {models.map((model, index) => (
                            <option key={index} value={model}>
                                {model}
                            </option>
                        ))}
                    </select>
                </div>
            </div>

            {/* Chat history */}
            <div className="w-full flex-grow overflow-y-auto p-2 border rounded-lg bg-gray-100 flex flex-col gap-2">
                {chatHistory.length === 0 ? (
                    <div
                        className="self-start text-left mr-auto"
                    >No messages yet.</div>
                ) : (
                    chatHistory.map((message, index) => (
                        <div
                            key={index}
                            className={`${message.role === "user" ?
                                "bg-blue-100 self-end text-right ml-auto border-blue-500" :
                                "bg-green-100 self-start text-left mr-auto border-green-500"}
                            } p-4 rounded-lg border-l-4`}
                        >
                            <strong
                                className="text-gray-800 font-bold">
                                {message.role === "user" ? "👤 User:" : "🤖 AI:"}
                            </strong>
                            <div className="prose">
                                <ReactMarkdown>{message.content}</ReactMarkdown>
                            </div>
                        </div>
                    ))
                )}
            </div>

            <div ref={chatEndRef} />

            {/* Input Area */}
            <div className="flex pt-2 w-full border-t">
                <input
                    className="flex-grow p-2 border rounded-l-lg text-base"
                    type="text"
                    value={userInput}
                    onChange={(e) => setUserInput(e.target.value)}
                    placeholder="Type your message..."
                    disabled={loading}
                    onKeyDown={(e) => {
                        if (e.key === "Enter") sendMessage();
                    }}
                />

                <button
                    className="px-4 py-2 bg-blue-600 text-white rounded-r-lg hover:bg-blue-700"
                    onClick={sendMessage}
                    disabled={loading} >
                    {loading ? "Generating..." : "Send"}
                </button>

                {loading && (
                    <button onClick={handleAbort} style={{ marginLeft: "10px", color: "red" }}>
                        Abort
                    </button>
                )}
            </div>
        </div>
    );
});

export default ChatWindow;