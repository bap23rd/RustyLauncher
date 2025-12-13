import { useEffect, useState } from 'react';
import './NotificationToast.css';

export interface Notification {
    id: string;
    message: string;
    type?: 'info' | 'success' | 'error';
    duration?: number;
}

interface NotificationToastProps {
    notification: Notification;
    onDismiss: (id: string) => void;
}

export function NotificationToast({ notification, onDismiss }: NotificationToastProps) {
    const duration = notification.duration || 4000;
    const [progress, setProgress] = useState(100);

    useEffect(() => {
        const startTime = Date.now();
        const interval = setInterval(() => {
            const elapsed = Date.now() - startTime;
            const remaining = Math.max(0, 1 - elapsed / duration);
            setProgress(remaining * 100);

            if (remaining <= 0) {
                clearInterval(interval);
                onDismiss(notification.id);
            }
        }, 16); // ~60fps

        return () => clearInterval(interval);
    }, [duration, notification.id, onDismiss]);

    return (
        <div
            className={`notification-toast ${notification.type || 'info'}`}
            onClick={() => onDismiss(notification.id)}
        >
            <div className="notification-content">
                {notification.message}
            </div>
            <div
                className="notification-timer"
                style={{ width: `${progress}%` }}
            />
        </div>
    );
}
