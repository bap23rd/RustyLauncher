# 🚀 RustyLauncher

**A desktop application for managing and launching different versions of Rust.**

## ❓ Why

Some people wanna play Rust **without wipes**!  
Or at least they want to play an *older version*.

This is for those of us that really don't have bags of time to dedicate to a months speedrun of Rust and prefer to sit back and build a farm to grow berries for teas ☕ they'll never use.

Or want a *change-up in game play style*. 🎮  
> I'm thinking this might allow for new dynamics, like **Anarchy**, **RP**, or **Event** based servers that can't operate with monthly updates and wipes.

That is where **RustyLauncher** comes in. ✨

- 📥 Download
- 🚀 Launch
- ⚙️ Manage
- 📦 Archive

## 🛠️ How

RustyLauncher provides a clean UI to allows the downloading of old Rust versions using a Depot Downloader backend that fetches from the Steam database and automagically 🪄 handles package downloads and merging to give you a ready-to-play Rust version.

It also manages your downloaded versions to make it *easy* to launch, delete, or archive them. 🗂️

All installs are kept separate from the Main Steam install.

## 📥 Install RustyLauncher

The install for RustyLauncher is fairly straight forward:

1. Head over to **releases** 🏷️
2. Find the **latest release** 🆕
3. Download the `RustyLauncher.Setup` exe 💾
4. Run the exe and follow the setup instructions to choose download location

> ⚠️ **Note:** You may get a Windows SmartScreen warning. This is just because this is my first app, and I am not going to pay out-of-pocket for a code-signing certificate.  
> Just select **"More Info"** and **"Run anyway"**. ✅

> ⚠️ **Requirement:** RustyLauncher requires the **.NET 8.0 Runtime**
        You can get it here: https://dotnet.microsoft.com/download/dotnet/8.0
            Scroll down a little to the latest .NET Runtime 8.0.xx and select the installer for your Windows.
            Run the installer and follow the steps, then install (or run if already installed) RustyLauncher.

To **update** the Launcher follow the normal install steps and select the *same location* it is installed to. 🔄  
It will identify the existing install, update the files, and leave your installs alone.

## Using RustyLauncher

RustyLauncher is straightforward to use.
Once installed it should be discoverable through the Windows Start menu and Search bar. 🔍
> **Bear in mind:** Installing versions installs the *full game*. Size will vary depending on the version. 💾

### 🏠 Home

First page you'll see. Select buttons to navigate to next pages.
Or read the latest rust news. Pulled from **Facepunch's RSS feed**. 📰

You'll also notice the **Quick Launch ribbon** at the bottom. This is present across *all* pages, and makes it easy to launch into your last played version. 🚀

### 📥 Download Version

Pick one of our **recommended versions**, or one of the notable **'other' versions**.
- A download window will open with most details auto-filled.
- Just add an install name, select the location, fill your **Steam login info**, and hit **'Start Download'**

> ⚠️ **Steam login info** is required to verify you own Rust. It is only shared with Steam (via SteamCMD) to fetch the downloads. It can also be saved in settings to autofill.

Alternatively, use the **'Custom Download'** button to open a window where all details can be *manually* filled.

### 📚 Library

This page will show all the versions you have downloaded.

You can launch with **Easy Anti-Cheat (EAC)** or without. Typically, and by default, EAC is required. But some old versions work better without it.

You can select the **bin icon** 🗑️ to delete a version, or the **settings icon** ⚙️ to change the name, view the file location, or archive the version.

**Archiving** a version will zip it up and move it to an archive folder. This will reduce its size, though not massively, so it is best practice to delete unplayed versions if you have limited space.
You can unarchive versions by selecting the **'Unarchive'** button in the library, and selecting the version to unarchive.

You can also use the **'Add Install Location'** button to have RustyLauncher discover existing installs. 📂

### ⚙️ Settings

Settings can be accessed by clicking the **gear icon** ⚙️ in the top right corner.

- The **General tab** shows a bit about your app, gives some support links, and has a couple scary buttons to **Delete** or **Archive** all installs.
- The **Steam Account tab** lets you set your Steam login info to autofill when downloading versions. 🔑
- And the **Install Paths tab** lets you set your default install location, and add existing install locations. 📁

## 🤝 Support and Suggestions

Please do reach out or open an **issue** or **pull request** on this GitHub! 🌟

## ⚖️ License and Disclaimers

In accordance with Depot Downloaders **GPL-2.0 license**, that is also what this project uses.

RustyLauncher is not affiliated with Facepunch, Steam, or Depot Downloader.
