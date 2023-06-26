package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
	"sync"
)

func PullRepositories(config *models.Config, nickname string, receiver *receiver.Receiver, allReposFlag bool) {
	if allReposFlag {
		var wg sync.WaitGroup
		for _, repo := range config.Repositories {
			wg.Add(1)
			go func(repo models.Repository) {
				defer wg.Done()
				pullChangesForRepository(&repo, config)
			}(repo)
		}
		wg.Wait()
	} else {
		repo := helpers.FindRepositoryByNickname(nickname, config)
		if repo != nil {
			pullChangesForRepository(repo, config)
		} else {
			fmt.Println("Repository not found in config.")
		}
	}
}

func pullChangesForRepository(repo *models.Repository, config *models.Config) {
	repoPath := helpers.GetRepositoryPath(repo, config)
	cmd := exec.Command("git", "-C", repoPath, "pull", "--all")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to pull changes for repository %s: %v\n", repo.Nickname, err)
	}
}
