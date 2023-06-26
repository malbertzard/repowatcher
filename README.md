# RepoWatcher

Repo Watch is a command-line tool for managing multiple Git repositories. It provides various commands to perform common tasks such as fetching changes from remote, pulling changes, cloning repositories, showing diffs, opening repositories in an IDE, and executing commands within repositories.

## Installation

To install Repo Watch, you need to have Go installed on your system. Follow these steps:

1. Clone the repository:

   ```bash
   git clone https://github.com/malbertzard/repowatcher.git
   ```

2. Change to the repowatcher directory:

   ```bash
   cd repowatcher
   ```

3. Build the executable:

   ```bash
   go build -o rw main.go
   ```

4. Add the executable to your system's PATH for easier access:

   - **Linux/macOS**: Move the `rw` executable to `/usr/local/bin` or any other directory listed in your PATH.

   - **Windows**: Add the directory containing the `rw` executable to your system's PATH.

## Usage

Repo Watch provides the following commands:

- `fetch`: Fetches changes from the remote repository for one or all repositories.

  ```bash
  rw fetch [nickname]
  ```

  - If `nickname` is provided, fetches changes for the repository with the specified nickname.
  - If no `nickname` is provided, fetches changes for all repositories.

- `list`: Lists all repositories configured in the `config.yaml` file.

  ```bash
  rw list
  ```

- `pull`: Pulls changes from the remote repository for one or all repositories.

  ```bash
  rw pull [nickname]
  ```

  - If `nickname` is provided, pulls changes for the repository with the specified nickname.
  - If no `nickname` is provided, pulls changes for all repositories.

- `clone`: Clones a repository or all repositories.

  ```bash
  rw clone [nickname]
  ```

- `diff`: Shows the diff for a repository or all repositories.

  ```bash
  rw diff [nickname]
  ```

- `rdiff`: Shows the diff for a repository from the remote.

  ```bash
  rw rdiff [nickname]
  ```

- `edit`: Opens a repository in an IDE.

  ```bash
  rw edit [nickname]
  ```
  - Opens the repository with the specified nickname in the configured IDE.

- `exec`: Executes a command in a repository.

  ```bash
  rw exec [nickname] [command]
  ```

  - WIP
  - Executes the specified command within the repository with the specified nickname.

For each command, you can use the `-c` or `--config` flag to specify a custom configuration file. By default, Repo Watch looks for a `config.yaml` file in the current directory.

## Configuration

The configuration file (`config.yaml`) contains the settings for Repo Watch. It should be placed in the same directory as the `repo-watch` executable. Here is an example configuration:

```yaml
---
rootFolder: /path/to/repositories
editCommand: code
repositories:
  - nickname: repo1
    folderName: repo1
    url: https://github.com/user/repo1.git
    sparse: true
  - nickname: repo2
    folderName: repo2
    url: https://github.com/user/re

po2.git
    sparse: false
```

- `rootFolder`: The root folder where the repositories will be cloned.
- `editCommand`: The command to open a repository in the IDE.
- `repositories`: A list of repositories to manage, each with the following properties:
  - `nickname`: A unique nickname for the repository.
  - `folderName`: The folder name for the cloned repository.
  - `url`: The URL of the remote Git repository.
  - `sparse`: Specifies whether the repository should be cloned sparsely (fetch only the current branch) or not.

## Examples

- Fetch changes from all repositories:

  ```bash
  rw fetch --all
  ```

- Clone a specific repository:

  ```bash
  rw clone repo1
  ```

- Open a repository in the configured IDE:

  ```bash
  rw edit repo2
  ```

- Execute a command within a repository:

  ```bash
  rw exec repo1 'go test ./...'
  ```

## Contributing

Contributions are welcome! If you find any issues or have suggestions for improvements, please open an issue or submit a pull request on the [GitHub repository](https://github.com/malbertzard/repowatcher).

## License

This project is licensed under the [MIT License](LICENSE).
