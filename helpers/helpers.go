package helpers

import (
	"fmt"
	"os"
	"path/filepath"
	"repo-watch/models"

	"gopkg.in/yaml.v2"
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

func LoadConfig(configFile string, config *models.Config) error {
	file, err := os.Open(configFile)
	if err != nil {
		return fmt.Errorf("failed to open config file: %v", err)
	}
	defer file.Close()

	decoder := yaml.NewDecoder(file)
	if err := decoder.Decode(&config); err != nil {
		return fmt.Errorf("failed to decode config file: %v", err)
	}

	return nil
}
