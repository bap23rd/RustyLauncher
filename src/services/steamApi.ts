export interface NewsItem {
    gid: string;
    title: string;
    url: string;
    is_external_url: boolean;
    author: string;
    contents: string;
    feedlabel: string;
    date: number;
    feedname: string;
    feed_type: number;
    appid: number;
}

export const getRustNews = async (): Promise<NewsItem[]> => {
    try {
        // Call through Electron IPC to avoid CORS
        return await window.electronAPI.getRustNews();
    } catch (error) {
        console.error('Error fetching Rust news:', error);
        return [];
    }
};
