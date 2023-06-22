package main

import (
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"
	"gopkg.in/yaml.v2"
)

var (
	rootCmd    = &cobra.Command{}
	config     = Config{}
	configPath string
)

const (
	configFileName = "config.yaml"
	exampleConfig  = `---
rootFolder: /path/to/root/folder
editCommand: code
repositories:
  - nickname: repo1
    url: https://github.com/user/repo1.git
    folderName: repo1
  - nickname: repo2
    url: https://github.com/user/repo2.git
    folderName: repo2
`
)

type Config struct {
	RootFolder   string       `yaml:"rootFolder"`
	EditCommand  string       `yaml:"editCommand"`
	Repositories []Repository `yaml:"repositories"`
}

type Repository struct {
	Nickname   string `yaml:"nickname"`
	URL        string `yaml:"url"`
	FolderName string `yaml:"folderName"`
}

func main() {
	rootCmd = &cobra.Command{
		Use:   "rw",
		Short: "A simple Git repository watcher",
	}

	rootCmd.PersistentFlags().StringVar(&configPath, "config", "", "Path to the config file")

	rootCmd.AddCommand(
		&cobra.Command{
			Use:   "list",
			Short: "List repositories",
			Run:   listRepositories,
		},
		&cobra.Command{
			Use:   "switch <nickname>",
			Short: "Switch to a repository",
			Args:  cobra.ExactArgs(1),
			Run:   switchToRepository,
		},
		&cobra.Command{
			Use:   "fetch",
			Short: "Fetch updates for repositories",
			Run:   fetchRepository,
		},
		&cobra.Command{
			Use:   "pull [nickname]",
			Short: "Pull changes for a repository",
			Args:  cobra.MaximumNArgs(1),
			Run:   pullChanges,
		},
		&cobra.Command{
			Use:   "ide <nickname>",
			Short: "Open repository in IDE",
			Args:  cobra.ExactArgs(1),
			Run:   openRepositoryInIDE,
		},
		&cobra.Command{
			Use:   "generate-example-config",
			Short: "Generate an example config file",
			Run:   generateExampleConfig,
		},
		&cobra.Command{
			Use:   "clone <nickname>",
			Short: "Clone a repository",
			Args:  cobra.ExactArgs(1),
			Run:   cloneRepository,
		},
		&cobra.Command{
			Use:   "help",
			Short: "Display help",
			Run:   displayHelp,
		},
		&cobra.Command{
			Use:   "execute <nickname> <command>",
			Short: "Execute a command in the repository's folder",
			Args:  cobra.MinimumNArgs(2),
			Run:   executeCommandInRepository,
		},
	)

	if err := rootCmd.Execute(); err != nil {
		log.Fatal(err)
	}
}

func loadConfig() error {
	if configPath == "" {
		homeDir, err := os.UserHomeDir()
		if err != nil {
			return fmt.Errorf("failed to get user's home directory: %v", err)
		}
		configPath = filepath.Join(homeDir, ".config", configFileName)
	}

	file, err := os.Open(configPath)
	if err != nil {
		return fmt.Errorf("failed to open config file: %v", err)
	}
	defer file.Close()

	decoder := yaml.NewDecoder(file)
	err = decoder.Decode(&config)
	if err != nil {
		return fmt.Errorf("failed to decode config file: %v", err)
	}

	return nil
}

func listRepositories(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	for _, repo := range config.Repositories {
		fmt.Printf("Nickname: %s\n", repo.Nickname)
		fmt.Printf("URL: %s\n", repo.URL)
		fmt.Printf("Folder Name: %s\n", repo.FolderName)
		fmt.Println("----------------------")
	}
}

func switchToRepository(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	nickname := args[0]
	repo := findRepositoryByNickname(nickname)
	if repo == nil {
		fmt.Println("Repository not found in config.")
		return
	}

	err := os.Chdir(getRepositoryPath(repo))
	if err != nil {
		fmt.Printf("Failed to switch to repository %s: %v\n", repo.Nickname, err)
	} else {
		fmt.Printf("Switched to repository: %s\n", repo.Nickname)
	}
}

func fetchRepository(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	if len(args) == 0 {
		fmt.Println("Please specify a repository nickname.")
		return
	}

	nickname := args[0]
	repo := findRepositoryByNickname(nickname)
	if repo == nil {
		fmt.Println("Repository not found in config.")
		return
	}

	fetchChanges(*repo)
}

func fetchChanges(repo Repository) {
	repoPath := getRepositoryPath(&repo)
	cmd := exec.Command("git", "-C", repoPath, "fetch")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to fetch changes for repository %s: %v\n", repo.Nickname, err)
	} else {
		fmt.Printf("Fetched changes for repository: %s\n", repo.Nickname)
	}
}

func pullChanges(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	if len(args) == 0 {
		fmt.Println("Please specify a repository nickname.")
		return
	}

	nickname := args[0]
	repo := findRepositoryByNickname(nickname)
	if repo == nil {
		fmt.Println("Repository not found in config.")
		return
	}

	pullChangesForRepository(*repo)
}

func pullChangesForRepository(repo Repository) {
	repoPath := getRepositoryPath(&repo)
	cmd := exec.Command("git", "-C", repoPath, "pull")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to pull changes for repository %s: %v\n", repo.Nickname, err)
	} else {
		fmt.Printf("Pulled changes for repository: %s\n", repo.Nickname)
	}
}

func openRepositoryInIDE(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	nickname := args[0]
	repo := findRepositoryByNickname(nickname)
	if repo == nil {
		fmt.Println("Repository not found in config.")
		return
	}

	repoPath := getRepositoryPath(repo)

	passCmd := exec.Command("bash", "-c", config.EditCommand)
	passCmd.Dir = repoPath
	passCmd.Stdout = os.Stdout
	passCmd.Stderr = os.Stderr
	err := passCmd.Run()
	if err != nil {
		fmt.Printf("Failed to execute command in repository %s: %v\n", repo.Nickname, err)
	}
}

func generateExampleConfig(cmd *cobra.Command, args []string) {
	fmt.Println(exampleConfig)
}

func cloneRepository(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	if len(args) == 0 {
		fmt.Println("Please specify a repository nickname.")
		return
	}

	nickname := args[0]
	repo := findRepositoryByNickname(nickname)
	if repo == nil {
		fmt.Println("Repository not found in config.")
		return
	}

	cloneRepositoryToPath(repo)
}

func cloneRepositoryToPath(repo *Repository) {
	repoPath := getRepositoryPath(repo)
	if _, err := os.Stat(repoPath); err == nil {
		fmt.Printf("Repository %s already exists.\n", repo.Nickname)
		return
	}

	cmd := exec.Command("git", "clone", repo.URL, repoPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to clone repository %s: %v\n", repo.Nickname, err)
	} else {
		fmt.Printf("Cloned repository: %s\n", repo.Nickname)
	}
}

func displayHelp(cmd *cobra.Command, args []string) {
	rootCmd.Help()
}

func executeCommandInRepository(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	if len(args) < 2 {
		fmt.Println("Please specify a repository nickname and a command.")
		return
	}

	nickname := args[0]
	repo := findRepositoryByNickname(nickname)
	if repo == nil {
		fmt.Println("Repository not found in config.")
		return
	}

	repoPath := getRepositoryPath(repo)

	// Join the command arguments with spaces and remove the leading/trailing single quotes
	command := strings.Join(args[1:], " ")
	command = strings.Trim(command, "'")

	passCmd := exec.Command("bash", "-c", command)
	passCmd.Dir = repoPath
	passCmd.Stdout = os.Stdout
	passCmd.Stderr = os.Stderr
	err := passCmd.Run()
	if err != nil {
		fmt.Printf("Failed to execute command in repository %s: %v\n", repo.Nickname, err)
	}
}

func findRepositoryByNickname(nickname string) *Repository {
	for _, repo := range config.Repositories {
		if repo.Nickname == nickname {
			return &repo
		}
	}
	return nil
}

func getRepositoryPath(repo *Repository) string {
	return filepath.Join(config.RootFolder, repo.FolderName)
}
