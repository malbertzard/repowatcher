package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
	"strings"
)

func DiffRepositories(config *models.Config, nickname string, receiver receiver.Receiver, allReposFlag bool)  {
	if allReposFlag {
		for _, repo := range config.Repositories {
			showRepositoryDiff(&repo, config)
		}
	} else {
		repo := helpers.FindRepositoryByNickname(nickname, config)
		if repo != nil {
			showRepositoryDiff(repo, config)
		} else {
			fmt.Println("Repository not found in config.")
		}
	}
}

func DiffRemoteRepositories(config *models.Config, nickname string, receiver receiver.Receiver, allReposFlag bool)  {
	if allReposFlag {
		for _, repo := range config.Repositories {
			showRemoteDiff(&repo, config)
		}
	} else {
		repo := helpers.FindRepositoryByNickname(nickname, config)
		if repo != nil {
			showRemoteDiff(repo, config)
		} else {
			fmt.Println("Repository not found in config.")
		}
	}
}

func showRepositoryDiff(repo *models.Repository, config *models.Config) {
	repoPath := helpers.GetRepositoryPath(repo, config)
	cmd := exec.Command("git", "-C", repoPath, "diff")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to show diff for repository %s: %v\n", repo.Nickname, err)
	}
}

func showRemoteDiff(repo *models.Repository, config *models.Config) {
	repoPath := helpers.GetRepositoryPath(repo, config)
	cmd := exec.Command("git", "-C", repoPath, "fetch", "--all")
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to fetch remote for repository %s: %v\n", repo.Nickname, err)
		return
	}

	cmd = exec.Command("git", "-C", repoPath, "rev-parse", "--abbrev-ref", "HEAD")
	output, err := cmd.Output()
	if err != nil {
		fmt.Printf("Failed to get current branch for repository %s: %v\n", repo.Nickname, err)
		return
	}
	branch := strings.TrimSpace(string(output))

    cmd = exec.Command("git", "-C", repoPath, "diff", branch, ("origin/"+ branch))
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err = cmd.Run()
	if err != nil {
		fmt.Printf("Failed to show remote diff for repository %s: %v\n", repo.Nickname, err)
	}
}

