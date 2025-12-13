import { createContext, useContext, useState, type ReactNode, useCallback, useMemo } from 'react';
import { type Notification, NotificationToast } from '../components/NotificationToast';

interface NotificationContextType {
    show: (message: string, type?: 'info' | 'success' | 'error', duration?: number) => void;
}

const NotificationContext = createContext<NotificationContextType | undefined>(undefined);

export function NotificationProvider({ children }: { children: ReactNode }) {
    const [notifications, setNotifications] = useState<Notification[]>([]);

    const show = useCallback((message: string, type: 'info' | 'success' | 'error' = 'info', duration: number = 4000) => {
        const id = Date.now().toString() + Math.random().toString(36).substr(2, 9);
        // Only set notifications if this creates a unique one? No, we trust the caller.
        const newNotification: Notification = { id, message, type, duration };

        setNotifications(prev => [...prev, newNotification]);
    }, []);

    const dismiss = useCallback((id: string) => {
        setNotifications(prev => prev.filter(n => n.id !== id));
    }, []);

    const value = useMemo(() => ({ show }), [show]);

    return (
        <NotificationContext.Provider value={value}>
            {children}
            <div className="notification-container">
                {notifications.map(n => (
                    <NotificationToast
                        key={n.id}
                        notification={n}
                        onDismiss={dismiss}
                    />
                ))}
            </div>
        </NotificationContext.Provider>
    );
}

export function useNotification() {
    const context = useContext(NotificationContext);
    if (!context) {
        throw new Error('useNotification must be used within a NotificationProvider');
    }
    return context;
}
