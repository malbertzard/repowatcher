# RepoWatcher

RepoWatcher is a command-line tool that watches your Git repositories for changes and performs various actions. This tool is written in Go and uses the Cobra and Viper libraries for command-line parsing and configuration management, respectively.

## Installation

To install RepoWatcher, you will need to have Go 1.16 or higher installed on your machine. Once you have installed Go, you can install RepoWatcher using the following command:

```sh
go install github.com/malbertzard/repowatcher@latest
```

This will download the source code for RepoWatcher and install it in your `$GOPATH/bin` directory.

## Usage

RepoWatcher has the following commands:

### `jump`

Changes the current working directory to the directory associated with the passed nickname.

```sh
repowatcher jump [nickname]
```

### `list`

Displays a list of all the nicknames and paths to the directories stored in the config.

```sh
repowatcher list
```

### `version`

Displays the version of RepoWatcher.

```sh
repowatcher version
```

## Configuration

RepoWatcher reads its configuration from a YAML file located at `$HOME/.repowatcher.yaml`. The configuration file contains the following keys:

```yaml
# Username for Git commands
username: johndoe

# Email address for Git commands
email: johndoe@example.com

# A collection of directories with nicknames
directories:
  home: /path/to/home/directory
  work: /path/to/work/directory
```

To override the location of the configuration file, you can specify the `--config` flag when running RepoWatcher:

```sh
repowatcher --config /path/to/config.yaml
```

## License

RepoWatcher is licensed under the [MIT License](https://opensource.org/licenses/MIT).
