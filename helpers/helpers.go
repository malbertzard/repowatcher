package helpers

import (
	"path/filepath"
	"repo-watch/models"
)

func GetRepositoryPath(repo *models.Repository, config *models.Config) string {
	return filepath.Join(config.RootFolder, repo.FolderName)
}

func FindRepositoryByNickname(nickname string, config *models.Config) *models.Repository {
	for _, repo := range config.Repositories {
		if repo.Nickname == nickname {
			return &repo
		}
	}
	return nil
}

func GetNicknameFromArgs(args []string) string {
	nickname := ""
	if len(args) > 0 {
		nickname = args[0]
	}
	return nickname
}
