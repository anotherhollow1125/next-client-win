## Download

[Releases page](https://github.com/anotherhollow1125/next-client-win/releases)

## Nextcloud Client for Windows

This is a [Nextcloud](https://nextcloud.com/) client application for Windows. It runs in the background, detects the addition, modification, deletion, etc. of files on the specified folder, and synchronizes bi-directionally with the folder on the server.

Of course, there is an official [client application](https://nextcloud.com/install/). However, the excluded file list feature does not seem to work well with this application, and even after setting the excluded file list, the application synchronizes by itself.

There is also an option to send a pull request to the official application (there is already an [issue](https://github.com/nextcloud/desktop/issues/2728)), but I didn't have the ability to write C++, so I used Rust instead. My motivation for creating this application in the first place was that I didn't want to synchronize the "target" folder in the Rust project.

I would be happy if this application is known to the Nextcloud official.

## 🚨 CAUTION 🚨

This app uses Basic Authentication to access your Nextcloud server. It means that if your server doesn't use SSL encryption, usernames and passwords will leak to attackers.

THEREFORE, **IF YOUR NEXTCLOUD SERVER's URL START "http://", YOU MUST NOT USE THIS APP!!!**

## How to use

### 1. install

Download the zip file from [here](https://github.com/anotherhollow1125/next-client-win/releases), unzip it, and place "next_client_win.exe" in an appropriate location for use. Double-click on it to launch it.

Using the Windows startup, this application is launched when Windows starts. Be sure to set this up.

ref: [Add an app to run automatically at startup in Windows 10](https://support.microsoft.com/en-us/windows/add-an-app-to-run-automatically-at-startup-in-windows-10-150da165-dcd9-7230-517b-cf3c295d89dd)

1. Create a shortcut for this application.
2. Launch "Run" with `Win key + R` and type `shell:startup`.
3. A folder will open. Put the shortcut of this application created in step 1 into the folder.

### 2. Initial launch

Make sure the machine is securely connected to the Internet before the first launch.

When you start the application for the first time, you will be asked to enter the necessary information. The information you enter will be saved in `conf.ini` of the folder where this application exists.

|Variable Name|Description|
|:-:|:---|
|NC_HOST| Enter the URL of your Nextcloud server. e.g. https: //cloud.example.com/ |
|NC_USERNAME| Enter the username. e.g. user |
|NC_PASSWORD| Enter the password. e.g. password |
|LOCAL_ROOT| Enter the path of the folder to be synchronized. e.g. c:/Users/user/Desktop/nextcloud |
|RUST_LOG| Set the level of logging. If omitted, the value is INFO. Choose from OFF, DEBUG, INFO, WARN, ERROR |

### 3. Icon in the notification area

#### 3.1. Icon types

When this application is launched, an icon indicating the status of this application will be placed in the notification area (The notification are is located in the lower right corner. For more information: https://www.computerhope.com/jargon/n/notiarea.htm )

There are 4 types of states.

|icon|Description|
|:-----:|:---|
| ![nc_normal](nc_normal.ico) | The normal icon. |
| ![nc_load](nc_load.ico) | The loading icon. This icon appears when the client communicating with the server or you are manipulating a folder. Heavy file manipulation during loading may result in folder corruption. |
| ![nc_offline](nc_offline.ico) | The offline icon. This icon appears when the machine is not connected to the network or the server is not accessible. Please check your network connection. The first time you start the program, you need to be connected to the Internet for sure for configuration. |
| ![nc_error](nc_error.ico) | The icon for errors. Use `show log` command to see what error is occurring, and fix it. |

#### 3.1. Commands

Click on the icon to display the context menu. You can use the commands in the context menu to operate the application.

|Command|Description|
|:-----:|:--|
|show log| Open the log file with notepad. You can also check the log from the file located in `.ncs/log`. |
|edit conf.ini| Open the configuration file of this application with notepad. |
|edit excludes| Open the `.ncs/excludes.json` file with notepad. You must use **regular expressions** to set which files to exclude and which files not to exclude. For details, see "5. Exclusion Settings". |
|repair| The folder will be modified so that its contents match those on the server. Files that exist only locally will be backed up to the `.ncs/stash` folder and then deleted. |
|restart| Restart this application. |
|exit| Exit this application. |

### 4. Metadata for synchronization

A hidden folder named `.ncs` will be created in the folder that is set as LOCAL_ROOT. The `.ncs` folder has the following structure. (The `stash` is created the first time a file is saved.)

```
.ncs/
  ├── log/
  ├── stash/
  ├── cache.json
  └── excludes.json
```

| Folger/File | Description |
|:--------------:|:--|
| log | Log files is stored here. The name of log files is the date when the application was launched. |
| stash | Folders and files deleted by the `repair` command are saved with the time appended to the file name. |
| cache.json | The cache data for synchronization is stored here. DO NOT EDIT IT. |
| excludes.json | Specify the folders / files you want to exclude from synchronization with ** regular expression **. For details, see "5. Exclusion settings". |

### 5. Exclusion Settings

`.ncs/excludes.json` is a JSON file consisting of the `blacks` and `wihtes` fields. You can open the file with the `edit excludes` command.

Set `blacks` to the folder/file names you don't want to sync, and `whites` to the folder/file names you want to sync even if they are trapped by `blacks`, using **regular expressions**. Folders/files that are determined not to be synchronized will be ignored even if they are created locally, and will not be saved locally even if they are saved on the server in some other way. Whitelists take precedence over blacklists, for example, if `\\d+\\.txt` is included in `blacks`, but `10.txt` is included in `whites`, then `10.txt` will be synchronized. Note that since the excludes is json file, `\` must be escaped to `\\`.

Because the check is done only for each folder/file name, it is not possible to check "I don't want to synchronize `hoge/target`, but I want to synchronize `fuga/target`". Please be aware of this point.

Folders/filenames starting with `.` and `~` are not synced by default, although this is not stated in `blacks` and `whites`.

Do not include `\\.ncs` in `whites`. The log file will keep being updated, resulting in an infinite loop and a heavy load on the server. (If you really want to synchronize, set `RUST_LOG` to `OFF`. The cost is that the log files will not be fully functional).

## Q&A

### Q1. Are local file operations recorded even when offline?

A1. Files that have been changed are recorded and will be synchronized when communication is restored, but you should refrain from moving folders or files, as they are not recorded. It should only be used when you are online.

### Q2. Folder corrupted!

A2. Try the `repair` command. If you still have problems, delete the entire contents of the folder, including the `.ncs` folder. (Note that this will also remove the `.ncs/excludes.json`, i.e. the exclusions setting).

If a file is lost due to an accidental operation, please don't give up hope it may still be in the trash on the server. Also, check the `.ncs/stash` folder for local files that have been deleted by using the `repair` command.

### Q3. Files aren't syncing! / Is the app not working in the first place?

A3. Didn't you shut down once? The "Automatically add this application to startup when first launched" function is not implemented. If it is not registered in the startup, you need to start it manually every time. Please register this application in the startup directory so that this application is launched when Windows starts.

### Q4. Some files are not synchronized! / Excluded files is synced!

A4. Check the following points.

- Files starting with `.` or `~` are considered hidden files and are not synchronized by default. Be especially careful with `.gitignore` files.
- All settings are regular expressions. For example, When you want to add `.gitignore` file to the whitelist, if you write `.gitignore`, it will be synced even if you have set `agitignore` to the blacklist. In this example, you need to write `\\.gitignore` . Using an expression like `^filename$`, which is a full match, is useful to prevent partial matches of folders and files from being synchronized.
- The whitelist takes precedence over the blacklist.
- It does not have the ability to judge by the entire path, but simply excludes/synchronizes files and folders that match the regular expression added to the blacklist/whitelist. If a parent folder is caught in the blacklist, its child files will not be synchronized.
- If you delete the `.ncs` folder for the purpose of resetting, `excludes.json` will also be deleted, so you will need to set it again.

### Q5. Some commands do not work.

A5. Some commands, such as `edit excludes`, do not work when clicked in case of an error. Please close the application and restart it.

### Q6. Unknown error occurred!

A6. If it is difficult to resolve, please create an issue. I'll do my best to help.

I'm Japanese, so I'll write a Japanese document below.

日本人なので以下に日本語のドキュメントも置いておきます。

## Nextcloud Client for Windows

[Nextcloud](https://nextcloud.com/)のWindows用クライアントアプリケーションです。バックグラウンドで動作し、設定したフォルダ上のファイルの追加、変更、削除等を検出してNextcloudのフォルダと双方向に同期します。

もちろん公式にも[クライアントアプリ](https://nextcloud.com/install/)は存在します。しかしこのアプリケーションでは除外ファイルリスト機能がうまく機能していないようで、除外ファイルの設定を行っても勝手に同期されてしまいます。

公式アプリに(すでに[issueはある](https://github.com/nextcloud/desktop/issues/2728)ようなので)プルリクエストでも送れば良かったのかもしれませんが、私にはC++を書く力はなかったため、代わりにRustで実装を行いました。そもそも、このアプリを作成した動機も「Rustプロジェクト内のtargetフォルダを同期させたくなかったから」です。

Nextcloud公式にこのアプリケーションの存在が知られたら嬉しいですね。

## 🚨 注意 🚨

このアプリはBasic認証を使用してNextcloudサーバーにアクセスします。つまり、SSL通信を用いない場合、ユーザー名とパスワードが攻撃者に漏洩します。

したがって、 **NextcloudサーバーのURLが"http://"で始まっているならばこのアプリケーションを絶対に使用しないでください。**

## 使い方

### 1. インストール

[ここ](https://github.com/anotherhollow1125/next-client-win/releases)からzipファイルをダウンロードした後解凍し、「next_client_win.exe」を適当な場所に置いて使用してください。ダブルクリックで起動します。

Windowsのスタートアップ機能を使用するとWindows起動時に本アプリも立ち上げてもらえます。必ず設定してください。

参考: [Windows 10 の起動時に自動的に実行するアプリを追加する](https://support.microsoft.com/ja-jp/windows/windows-10-%E3%81%AE%E8%B5%B7%E5%8B%95%E6%99%82%E3%81%AB%E8%87%AA%E5%8B%95%E7%9A%84%E3%81%AB%E5%AE%9F%E8%A1%8C%E3%81%99%E3%82%8B%E3%82%A2%E3%83%97%E3%83%AA%E3%82%92%E8%BF%BD%E5%8A%A0%E3%81%99%E3%82%8B-150da165-dcd9-7230-517b-cf3c295d89dd)

1. 本アプリのショートカットを作成する
2. `Winキー + R` で「ファイル名を指定して実行」を起動し、 `shell:startup` と入力する
3. フォルダが開くので1で作成した本アプリのショートカットをそのフォルダに入れる

### 2. 初回起動時

初回起動前にマシンがインターネットに確実に接続されていることを確認してください。

初回起動時には、必要な情報の入力を求められます。入力した情報は本アプリが存在するフォルダの `conf.ini` に保存されます。

|項目|内容|
|:-:|:---|
|NC_HOST| あなたのNextcloudサーバーのURLを入力してください。 例: https: //cloud.example.com/ |
|NC_USERNAME| ユーザー名を入力してください。 例: user |
|NC_PASSWORD| パスワードを入力してください。 例: password |
|LOCAL_ROOT| 同期させるフォルダのパスを入力してください。 例: c:/Users/user/Desktop/nextcloud |
|RUST_LOG| 出力されるログのレベルを設定できます。省略した場合はINFOです。 OFF, DEBUG, INFO, WARN, ERROR から選べます。 |

### 3. 通知領域アイコン

#### 3.1. アイコンの種類

本アプリを起動すると、通知領域(あるいはタスクトレイ等と呼ばれます。[参考](https://atmarkit.itmedia.co.jp/ait/articles/1604/19/news009.html))に本アプリの状態を示すアイコンが設置されます。

状態は4種類存在します。

|アイコン|説明|
|:-----:|:---|
| ![nc_normal](nc_normal.ico) | 正常時のアイコンです。 |
| ![nc_load](nc_load.ico) | ロード中のアイコンです。サーバーと通信中であったり、フォルダを操作している時にこのアイコンになります。ロード中に激しくファイル操作を行った場合、フォルダが破損する可能性があります。 |
| ![nc_offline](nc_offline.ico) | オフラインのアイコンです。マシンがネットワークに接続されていなかったり、サーバーにアクセスできない時にこのアイコンになります。ネットワークの接続を確認してください。また、初回起動時は設定のために確実にインターネットに接続されている必要があります。 |
| ![nc_error](nc_error.ico) | エラー時のアイコンです。 `show log` でどのようなエラーが発生しているかを確認し、直してください。 |

#### 3.1. アイコンによる操作 (コマンド)

アイコンをクリックするとコンテキストメニューが表示されます。コンテキストメニューにあるコマンドからアプリの操作が行えます。

|コマンド|説明|
|:-----:|:--|
|show log| ログファイルをnotepadで起動します。ログは `.ncs/log` に存在するファイルからも確認できます。 |
|edit conf.ini| 本アプリの設定ファイルをnotepadで起動します。 |
|edit excludes| `.ncs/excludes.json` ファイルをnotepadで起動します。 **正規表現で** 除外するファイル、除外しないファイルを設定できます。詳しくは「5. 除外設定」を確認してください。 |
|repair|フォルダの内容がサーバー上のものと一致するように修正を行います。ローカル上にのみ存在するファイルは、 `.ncs/stash` フォルダにバックアップを取った上で消去されます。|
|restart|本アプリを再起動します。|
|exit|本アプリを終了します。|

### 4. 同期用メタデータ

同期対象として設定されたフォルダには `.ncs` という隠しフォルダが生成されます。 `.ncs` フォルダは以下のような構成になっています。( `stash` は初めてファイルの退避が行われる時に生成されます。)

```
.ncs/
  ├── log/
  ├── stash/
  ├── cache.json
  └── excludes.json
```

|フォルダ/ファイル|説明|
|:--------------:|:--|
|log| ログファイルが格納されています。ログファイルの名前はアプリ起動時の日付となっています。 |
|stash| `repair` コマンド等で削除されたフォルダやファイルが時刻をファイル名の後ろにつけた形で退避されています。 |
|cache.json| 同期用のキャッシュデータが格納されています。編集しないでください。 |
|excludes.json| 同期から除外したいフォルダ/ファイルを **正規表現** で指定します。詳しくは「5. 除外設定」を確認してください。 |

### 5. 除外設定

`.ncs/excludes.json` は `blacks` フィールドと `wihtes` フィールドからなるJSONファイルです。 `edit excludes` コマンドからファイルを開くことができます。

`blacks` には同期したくないフォルダ/ファイル名を、 `whites` には `blacks` に引っかかるものの同期を行いたいフォルダ/ファイル名を、それぞれ **正規表現で** 設定します。同期しないと判断されたフォルダ/ファイルは、ローカルで作成されても無視され、別な方法でサーバー上に保存されてもローカルに保存されません。ブラックリストよりホワイトリストが優先され、例えば `\\d+\\.txt` を `blacks` に含めていても、 `whites` に `10.txt` が含まれていれば `10.txt` は同期されます。excludesファイルはjsonファイルであるため、`\` は `\\` へとエスケープする必要性があることに注意してください。

あくまでも各フォルダ/ファイル名に対してのみチェックを行うので、「 `hoge/target` は同期したくないけど `fuga/target` は同期したい」というような設定は不可能です。ご了承ください。

`.` と `~` で始まるフォルダ/ファイル名は、 `blacks` 、 `whites` には明記されていませんがデフォルトで同期されません。

`whites` に `\\.ncs` を含めることだけは絶対にやめてください。ログファイルが更新され続けるため無限ループとなりサーバーに多大な負荷がかかります。(どうしても同期したければ `RUST_LOG` を `OFF` に設定してください。その代償としてログファイルは完全に機能しません。)

## Q&A

### Q1. オフライン時もローカルでのファイル操作は記録されていますか？

A1. 変更があったファイルの記録は行っており、通信回復時に同期されますが、フォルダやファイルの移動などは記録していないため控えたほうが良いです。あくまでもオンライン時に使用するようにしてください。

### Q2. フォルダが壊れました

A2. `repair` コマンドを試してください。それでも不具合がある場合は、フォルダの中身を `.ncs` フォルダを含めすべて消去してください。(その場合、 `.ncs/excludes.json` 、すなわち除外設定も削除されることに気をつけてください。)

誤った操作でファイル等が消えた場合、サーバーの方のゴミ箱に残っている可能性があるので望みを捨てないでください。また、 `repair` コマンドを使用した場合などで消去されたローカルのファイルは `.ncs/stash` フォルダに保存されている可能性があるので、そちらも合わせて確認してください。

### Q3. ファイルが同期されない！/そもそもアプリが働いていない？

A3. 一度シャットダウンしませんでしたか？「初回起動時にスタートアップに本アプリを自動的に追加する」機能は実装されていません。スタートアップに登録しない場合毎回手動で起動する必要があります。「使い方」の1. インストールを参考に本アプリをスタートアップに登録し、Windows起動時に本アプリが起動するようにしてください。

### Q4. 一部ファイルが同期されない！/除外設定したファイルが同期される！

A4. 次の点を確認してください。

- `.` 、 `~` で始まるファイルは隠しファイルとみなしデフォルトで同期されません。特に `.gitignore` ファイルなどは注意が必要となります。
- 設定はすべて正規表現です。例えば `.gitignore` ファイルをホワイトリストに加えたい場合、`.gitignore` と書いてしまうと `agitignore` 等をブラックリストに設定していても同期されてしまいます。この例では `\\.gitignore` と書く必要があります。フルマッチとなる `^filename$` のような表現を使うと、部分マッチのフォルダやファイルが同期されるのを防ぐことができ便利です。
- ブラックリストよりホワイトリストが優先されます。
- パス全体で判断する機能はなく、単純にブラックリスト/ホワイトリストに追加された正規表現にマッチするファイル/フォルダは排除/同期されます。親フォルダがブラックリストに引っかかった場合、その子ファイルは同期されません。ご注意ください。
- リセット等を目的として `.ncs` フォルダを消去してしまった場合、 `excludes.json` も削除されるため、改めて設定する必要があります。

### Q5. 一部のコマンドが機能しない

A5. `edit excludes` 等一部コマンドはエラー時にはクリックしても何も起きません。一度アプリを閉じ、再起動してください。

### Q6. 意味不明なエラーが発生した！

A6. 解決困難であればissueを立ててください。できる限り対応します。